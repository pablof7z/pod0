//! Minimal local RSS server for headless scenario tests.
//!
//! `start()` binds a TcpListener on 127.0.0.1:0 (OS-assigned port), spawns a
//! background thread that accepts **one** HTTP connection and responds with a
//! hand-crafted 3-episode RSS feed, then exits. Returns the bound port so the
//! caller can construct `http://127.0.0.1:{port}/feed.xml`.
//!
//! The feed is intentionally minimal: only the fields the Podcast kernel
//! actually parses (title, enclosure, itunes:duration). Three episodes is
//! enough to satisfy all `rss_subscribe` assertions while keeping parse time
//! well under 100 ms even in debug mode.

use std::io::{Read as IoRead, Write as IoWrite};
use std::net::TcpListener;

/// RSS XML served by the mock server. Three episodes with distinct titles and
/// valid `itunes:duration` values (seconds as integer, widely supported).
const FEED_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"
     xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd">
  <channel>
    <title>Mock Podcast</title>
    <link>http://127.0.0.1</link>
    <description>Headless test feed</description>
    <item>
      <title>Episode One</title>
      <enclosure url="http://127.0.0.1/ep1.mp3" length="1000000" type="audio/mpeg"/>
      <itunes:duration>1800</itunes:duration>
    </item>
    <item>
      <title>Episode Two</title>
      <enclosure url="http://127.0.0.1/ep2.mp3" length="2000000" type="audio/mpeg"/>
      <itunes:duration>2400</itunes:duration>
    </item>
    <item>
      <title>Episode Three</title>
      <enclosure url="http://127.0.0.1/ep3.mp3" length="1500000" type="audio/mpeg"/>
      <itunes:duration>3000</itunes:duration>
    </item>
  </channel>
</rss>
"#;

/// Start the mock RSS server. Returns the OS-assigned port number.
///
/// The server accepts exactly one HTTP request, writes the feed response, then
/// the background thread exits. The `TcpListener` is closed after `accept()`.
pub fn start() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("mock_feed: bind");
    let port = listener.local_addr().expect("mock_feed: local_addr").port();

    std::thread::spawn(move || {
        // Accept the single connection the capability host will make.
        if let Ok((mut stream, _)) = listener.accept() {
            // Drain the HTTP request headers (reqwest sends a proper GET).
            // We only need to consume enough bytes to unblock the client write.
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);

            let body = FEED_XML.as_bytes();
            let response = format!(
                "HTTP/1.1 200 OK\r\n\
                 Content-Type: application/rss+xml; charset=utf-8\r\n\
                 Content-Length: {}\r\n\
                 Connection: close\r\n\
                 \r\n",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(body);
            // stream drops here, closing the connection cleanly.
        }
    });

    port
}
