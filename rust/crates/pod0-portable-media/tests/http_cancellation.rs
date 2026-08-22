use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use pod0_portable_media::{
    CancellationToken, HttpLoadOptions, MediaError, MediaLoader, MediaSource,
};

#[test]
fn cancellation_interrupts_a_stalled_http_body_promptly() {
    let (url, headers_ready, release_server, server) = serve_stalled_body();
    let cancellation = CancellationToken::new();
    let load_cancellation = cancellation.clone();
    let load = thread::spawn(move || {
        MediaLoader::new(HttpLoadOptions::default())
            .expect("build HTTP client")
            .load(MediaSource::Http(url), &load_cancellation)
    });

    headers_ready
        .recv_timeout(Duration::from_secs(2))
        .expect("server sent response headers");
    thread::sleep(Duration::from_millis(100));
    let cancelled_at = Instant::now();
    cancellation.cancel();
    let result = load.join().expect("HTTP load thread");
    let cancellation_latency = cancelled_at.elapsed();
    release_server.send(()).expect("release server");
    server.join().expect("stalling HTTP server");

    assert!(matches!(result, Err(MediaError::Cancelled)));
    assert!(
        cancellation_latency < Duration::from_secs(1),
        "cancellation took {cancellation_latency:?}"
    );
}

fn serve_stalled_body() -> (
    url::Url,
    mpsc::Receiver<()>,
    mpsc::Sender<()>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind HTTP fixture");
    let address = listener.local_addr().expect("HTTP fixture address");
    let (headers_sender, headers_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept HTTP request");
        let mut request = [0_u8; 2_048];
        let _ = stream.read(&mut request).expect("read HTTP request");
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: 4096\r\n\
             Connection: close\r\n\r\n"
        )
        .expect("write response headers");
        stream.flush().expect("flush response headers");
        headers_sender.send(()).expect("signal response headers");
        release_receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("test releases stalling server");
    });
    (
        url::Url::parse(&format!("http://{address}/stalled.wav")).expect("fixture URL"),
        headers_receiver,
        release_sender,
        handle,
    )
}
