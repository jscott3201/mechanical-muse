//! Serving the built web app (`web/dist`) from the same process as the API, so a demo is one
//! command and one address. Only files under the configured folder are served; any other path
//! that is not under `/api` gets `index.html` (the app routes in the browser).

use std::path::{Component, Path, PathBuf};

use axum::body::Body;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};

/// The file for `uri` under `root`, or `None` when the path leaves `root` or names nothing.
fn resolve(root: &Path, uri: &Uri) -> Option<PathBuf> {
    let rel = uri.path().trim_start_matches('/');
    let mut path = root.to_path_buf();
    for part in Path::new(rel).components() {
        match part {
            Component::Normal(p) => path.push(p),
            // `..`, a root or a drive prefix could leave the folder: refuse.
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
            Component::CurDir => {}
        }
    }
    path.is_file().then_some(path)
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// The app's file for `uri`, or `index.html` for an app route. `/api/...` never reaches here.
pub async fn serve(root: PathBuf, uri: Uri) -> Response {
    let file = resolve(&root, &uri).or_else(|| {
        let index = root.join("index.html");
        index.is_file().then_some(index)
    });
    let Some(file) = file else {
        return (
            StatusCode::NOT_FOUND,
            "the web app is not built (cd web && npm run build)",
        )
            .into_response();
    };
    let path = file.clone();
    let read = tokio::task::spawn_blocking(move || std::fs::read(path)).await;
    match read {
        Ok(Ok(bytes)) => {
            let mut response = Response::new(Body::from(bytes));
            let headers = response.headers_mut();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static(content_type(&file)),
            );
            // Hashed assets can be cached; index.html must not be, so a rebuild shows at once.
            let cache = if file.ends_with("index.html") {
                "no-cache"
            } else {
                "public, max-age=31536000, immutable"
            };
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
            response
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "could not read the file").into_response(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn paths_cannot_leave_the_folder() {
        let root = std::env::temp_dir().join(format!("mm-web-{}", std::process::id()));
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("index.html"), "<html>").unwrap();
        std::fs::write(root.join("assets/app.js"), "x").unwrap();
        let uri = |s: &str| s.parse::<Uri>().unwrap();
        assert_eq!(
            resolve(&root, &uri("/assets/app.js")),
            Some(root.join("assets/app.js"))
        );
        assert_eq!(resolve(&root, &uri("/")), None, "a folder is not a file");
        assert_eq!(resolve(&root, &uri("/../etc/passwd")), None);
        assert_eq!(resolve(&root, &uri("/assets/../../x")), None);
        assert_eq!(
            content_type(Path::new("a.js")),
            "text/javascript; charset=utf-8"
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
