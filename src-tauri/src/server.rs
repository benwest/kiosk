use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub struct ServerHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ServerHandle {
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub fn start(root: String, port: u16) -> Result<ServerHandle, String> {
    let server = tiny_http::Server::http(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();
    let root = PathBuf::from(root);

    let thread = thread::spawn(move || {
        while !stop_clone.load(Ordering::Relaxed) {
            match server.recv_timeout(Duration::from_millis(200)) {
                Ok(Some(request)) => handle_request(request, &root),
                Ok(None) => continue,
                Err(_) => break,
            }
        }
    });

    Ok(ServerHandle {
        stop,
        thread: Some(thread),
    })
}

fn handle_request(request: tiny_http::Request, root: &Path) {
    let url_path = request.url().split('?').next().unwrap_or("/");
    let rel = url_path.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };

    let mut file_path = root.to_path_buf();
    for component in Path::new(rel).components() {
        match component {
            std::path::Component::Normal(part) => file_path.push(part),
            std::path::Component::CurDir => {}
            // Reject any component that could escape root (ParentDir, RootDir, Prefix).
            _ => {
                let _ = request.respond(tiny_http::Response::empty(403));
                return;
            }
        }
    }

    if file_path.is_dir() {
        file_path.push("index.html");
    }

    match std::fs::File::open(&file_path) {
        Ok(mut file) => {
            let mut data = Vec::new();
            if file.read_to_end(&mut data).is_ok() {
                let header = tiny_http::Header::from_bytes(
                    &b"Content-Type"[..],
                    content_type(&file_path).as_bytes(),
                )
                .unwrap();
                let _ = request.respond(tiny_http::Response::from_data(data).with_header(header));
            } else {
                let _ = request.respond(tiny_http::Response::empty(500));
            }
        }
        Err(_) => {
            let _ = request.respond(tiny_http::Response::empty(404));
        }
    }
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css",
        "js" | "mjs" => "application/javascript",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}
