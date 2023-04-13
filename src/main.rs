use actix_web::{post, web, App, HttpServer, Responder};
use bollard::Docker;
use rusqlite::Connection;
use serde::Deserialize;
use std::sync::{Arc, Mutex};
mod actions;
use actions::*;
mod proxy;
use proxy::*;

#[derive(Clone)]
pub struct AppState {
    arc_conn: Arc<Mutex<Connection>>,
    docker: Arc<Docker>,
}

#[derive(Deserialize)]
struct Info {
    container_id: Option<String>,
    port: Option<String>,
}

#[post("/docker/{action}")]
async fn manage_docker(
    data: web::Data<AppState>,
    query: web::Query<Info>,
    path: web::Path<String>,
) -> impl Responder {
    let action = path.into_inner();
    let container_id = query.container_id.clone().unwrap_or("".to_owned());
    let port = query.port.clone();

    println!("Action: {} for container: {}", action, container_id.clone());

    match action.as_str() {
        "add" => add_container(data, &container_id, port).await,
        "start" => start(data, &container_id).await,
        "stop" => stop(data, &container_id).await,
        "remove" => remove_container(data, &container_id).await,
        "list" => list_containers(data).await,
        _ => format!("Unsupported action: {}", action),
    }
}

/// Main entry point for the web server.
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // connect to sqlite
    let conn = Connection::open("containers.db").unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS key_value_store (
            key TEXT PRIMARY KEY,
            value TEXT
        );",
        [],
    )
    .unwrap();
    let arc_conn = Arc::new(Mutex::new(conn));

    // connect to docker
    let docker = Docker::connect_with_local_defaults().unwrap();
    let docker_arc = Arc::new(docker);

    // create shared state
    let app_state = web::Data::new(AppState {
        arc_conn: arc_conn.clone(),
        docker: docker_arc.clone(),
    });

    // start http server
    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .service(manage_docker)
            .service(web::resource("/proxy/{container_id}/{path:.*}").route(web::to(reverse_proxy)))
    })
    .bind("0.0.0.0:8081")?
    .run()
    .await
}
