#![allow(dead_code)]

use std::{
    fs,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError},
    },
    thread,
    time::{Duration, Instant},
};

static DIRECTORY_COUNTER: AtomicU64 = AtomicU64::new(1);

pub struct TcpTestServer {
    address: String,
    accepted: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    done: Receiver<()>,
    thread: Option<thread::JoinHandle<()>>,
}

impl TcpTestServer {
    pub fn spawn(
        connections: usize,
        handler: impl Fn(usize, &str) -> Vec<u8> + Send + Sync + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local TCP test server");
        let address = listener
            .local_addr()
            .expect("read local address")
            .to_string();
        listener
            .set_nonblocking(true)
            .expect("set test listener nonblocking");
        let handler = Arc::new(handler);
        let accepted = Arc::new(AtomicUsize::new(0));
        let thread_accepted = Arc::clone(&accepted);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let (done_tx, done) = mpsc::channel();
        let thread = thread::spawn(move || {
            for index in 0..connections {
                let Some(mut stream) = accept_before_deadline(&listener, &thread_stop) else {
                    let _ = done_tx.send(());
                    return;
                };
                thread_accepted.fetch_add(1, Ordering::AcqRel);
                stream
                    .set_nonblocking(false)
                    .expect("set accepted test stream blocking");
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .expect("set read timeout");
                let Some(request) = read_request(&mut stream) else {
                    continue;
                };
                let response = handler(index, &request);
                let _ = stream.write_all(&response);
                let _ = stream.flush();
                // Half-close after flushing so the client observes an orderly HTTP EOF.
                // A full shutdown can reset the socket before buffered response bytes are
                // consumed, which made otherwise valid responses fail only under suite load.
                let _ = stream.shutdown(Shutdown::Write);
            }
            let _ = done_tx.send(());
        });
        Self {
            address,
            accepted,
            stop,
            done,
            thread: Some(thread),
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.address, path)
    }

    pub fn accepted_connections(&self) -> usize {
        self.accepted.load(Ordering::Acquire)
    }
}

impl Drop for TcpTestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            match self.done.recv_timeout(Duration::from_secs(3)) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                    thread.join().expect("local TCP test server failed");
                }
                Err(RecvTimeoutError::Timeout) if !thread::panicking() => {
                    drop(thread);
                    panic!("local TCP test server did not stop before deadline");
                }
                Err(RecvTimeoutError::Timeout) => drop(thread),
            }
        }
    }
}

fn accept_before_deadline(listener: &TcpListener, stop: &AtomicBool) -> Option<TcpStream> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if stop.load(Ordering::Acquire) {
            return None;
        }
        match listener.accept() {
            Ok((stream, _)) => return Some(stream),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "accept test connection before deadline"
                );
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("accept test connection: {error}"),
        }
    }
}

fn read_request(stream: &mut TcpStream) -> Option<String> {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1_024];
    loop {
        let count = match stream.read(&mut buffer) {
            Ok(count) => count,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::UnexpectedEof
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::WouldBlock
                ) =>
            {
                return None;
            }
            Err(error) => panic!("read HTTP request: {error}"),
        };
        if count == 0 {
            return None;
        }
        request.extend_from_slice(&buffer[..count]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            return Some(String::from_utf8(request).expect("test request is UTF-8 HTTP"));
        }
    }
}

pub fn response(status: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut response = format!("HTTP/1.1 {status}\r\n");
    for (name, value) in headers {
        response.push_str(name);
        response.push_str(": ");
        response.push_str(value);
        response.push_str("\r\n");
    }
    response.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    ));
    let mut bytes = response.into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

pub struct TestDirectory(PathBuf);

impl TestDirectory {
    pub fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".test-artifacts")
            .join(format!(
                "{}-{}",
                std::process::id(),
                DIRECTORY_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&path).expect("create project-local test directory");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
        if let Some(parent) = self.0.parent() {
            let _ = fs::remove_dir(parent);
        }
    }
}

#[test]
fn client_disconnect_before_headers_is_clean_shutdown() {
    let server = TcpTestServer::spawn(1, |_index, _request| {
        panic!("incomplete request must not reach the handler")
    });
    let stream = TcpStream::connect(&server.address).expect("connect to local TCP test server");
    let deadline = Instant::now() + Duration::from_secs(1);
    while server.accepted_connections() == 0 {
        assert!(
            Instant::now() < deadline,
            "server must accept fixture connection"
        );
        thread::yield_now();
    }
    stream
        .shutdown(Shutdown::Both)
        .expect("disconnect fixture client");
    drop(server);
}
