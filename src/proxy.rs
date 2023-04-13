use crate::AppState;
use actix_web::{
    http::Method,
    web::Bytes,
    HttpRequest, {web, Responder},
};
use rusqlite::params;

pub async fn reverse_proxy(
    req: HttpRequest,
    data: web::Data<AppState>,
    path: web::Path<(String, String)>,
    body: Bytes,
) -> impl Responder {
    let (container_id, sub_path) = path.into_inner();

    let conn = data.arc_conn.lock().unwrap();

    let mut stmt = conn
        .prepare("SELECT key, value FROM key_value_store WHERE key = ?")
        .unwrap();

    println!("container_id: {}", container_id);

    let mut rows = stmt
        .query_map(params![format!("{}/port", container_id.clone())], |row| {
            //
            let key: String = row.get(0)?;
            let value: String = row.get(1)?;
            Ok((key, value))
        })
        .unwrap();

    // get first row and exit if no rows
    let row = match rows.next() {
        Some(row) => row,
        None => return format!("No container found for ID: {}", container_id),
    };

    let (key, port) = row.unwrap();
    println!("key: {}, value: {}", key, port);

    let target_url = format!("http://{}:{}/{}", "127.0.0.1", port, sub_path);
    println!("Proxying request to: {}", target_url);

    let client = reqwest::Client::new();
    let result = match *req.method() {
        Method::GET => {
            // Handle GET request
            client.get(target_url).send().await
        }
        Method::POST => {
            // Handle POST request with body
            client.post(target_url).body(body.to_vec()).send().await
        }
        _ => {
            // Handle unsupported methods
            return format!("Unsupported method: {}", req.method());
        }
    };

    match result {
        Ok(response) => {
            let _status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| String::from("Error reading body"));
            return body;
        }
        Err(e) => return format!("Error proxying request: {}", e),
    }
}
