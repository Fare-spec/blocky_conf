use actix_web::{get, post, web, App, HttpResponse, HttpServer, Responder};
use actix_web::middleware::Logger;
use serde::Deserialize;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tokio::time::sleep;

/// État partagé : HTML en mémoire + timestamp du dernier reload
struct AppState {
    html: RwLock<String>,
    last_modified: RwLock<SystemTime>,
}

#[get("/")]
async fn index(data: web::Data<Arc<AppState>>) -> actix_web::Result<impl Responder> {
    let html = data.html.read().await;
    Ok(
        HttpResponse::Forbidden()
            .content_type("text/html; charset=utf-8")
            .body(html.clone())
    )
}

#[derive(Deserialize)]
struct Report { url: String }

#[post("/report")]
async fn report(report: web::Json<Report>) -> actix_web::Result<impl Responder> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("usr/local/share/blocked_page/urls/reported_urls.txt")
        .map_err(|e| {
            eprintln!("error while opening report file: {}", e);
            actix_web::error::ErrorInternalServerError("Failed to write report")
        })?;
    writeln!(file, "{}", report.url).map_err(|e| {
        eprintln!("error while writing to report file: {}", e);
        actix_web::error::ErrorInternalServerError("Failed to write report")
    })?;
    Ok(HttpResponse::Ok().body("URL reportée"))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));
    let path = "/usr/local/share/forbidden.html";
    let metadata = fs::metadata(path)?;
    let modified_time = metadata.modified().unwrap_or(SystemTime::now());
    let initial_html = fs::read_to_string(path).unwrap_or_default();

    let state = Arc::new(AppState {
        html: RwLock::new(initial_html),
        last_modified: RwLock::new(modified_time),
    });

    {
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(10)).await;
                if let Ok(meta) = fs::metadata(path) {
                    if let Ok(mod_time) = meta.modified() {
                        let mut last = state.last_modified.write().await;
                        if mod_time > *last {
                            if let Ok(new_html) = fs::read_to_string(path) {
                                *state.html.write().await = new_html;
                                *last = mod_time;
                                log::info!("forbidden.html rechargé à {:?}", mod_time);
                            }
                        }
                    }
                }
            }
        });
    }

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(Arc::clone(&state)))
            .wrap(Logger::default())
            .service(index)
            .service(report)
    })
    .bind(("0.0.0.0", 80))?
    .workers(num_cpus::get())
    .run()
    .await
}
