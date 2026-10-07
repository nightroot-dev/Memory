//! Sidecar server round-trip (M5): protocol, batching over the wire, bad input.

use cmhm_sidecar::SidecarClient;

async fn spawn_server() -> (String, tokio::task::JoinHandle<()>) {
    // Bind here (port 0) and hand the listener over: no free-port race.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let batcher = cmhm_sidecar::Batcher::new(Box::new(cmhm_sidecar::HashBackend));
    let h = tokio::spawn(async move {
        cmhm_sidecar::server::serve_on(listener, batcher).await.unwrap();
    });
    (addr, h)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn round_trip_and_bad_input() {
    let (addr, _srv) = spawn_server().await;
    let client = SidecarClient::new(addr.clone());
    let resp = client
        .embed(vec!["hello world".to_string(), "other".to_string()])
        .unwrap();
    assert_eq!(resp.model, "hash-128d");
    assert_eq!(resp.dim, 128);
    assert_eq!(resp.embeddings.len(), 2);
    assert_eq!(resp.embeddings[0].len(), 128);

    // Malformed line gets an error object, connection stays usable.
    let stream = std::net::TcpStream::connect(&addr).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    use std::io::{BufRead, BufReader, Write};
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = stream;
    writer.write_all(b"not json\n").unwrap();
    writer.flush().unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert!(line.contains("error"), "bad input must get error object: {line}");
    // Same connection still works.
    writer.write_all(b"{\"embed\":[\"again\"]}\n").unwrap();
    writer.flush().unwrap();
    line.clear();
    reader.read_line(&mut line).unwrap();
    assert!(line.contains("hash-128d"), "connection must survive bad input: {line}");
}
