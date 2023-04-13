use actix_web::{post, web, App, HttpServer, Responder};
use bollard::container::{Config, CreateContainerOptions};
use bollard::{
    service::{HostConfig, PortBinding},
    Docker,
};
use rusqlite::Connection;
use serde::Deserialize;
use std::{
    collections::HashMap,
    env,
    sync::{Arc, Mutex},
};

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

async fn add_container(
    data: web::Data<AppState>,
    container_id: &String,
    port: Option<String>,
) -> String {
    let env_image_name = env::var("IMAGE_NAME").unwrap_or("".to_string());
    let image_name = env_image_name.as_str();

    let options = CreateContainerOptions {
        name: container_id.clone(),
        ..Default::default()
    };

    // insert into sqlite in the key_value_store
    let conn = data.arc_conn.lock().unwrap();

    // save port
    conn.execute(
        r#"
            INSERT INTO key_value_store (key, value) 
            VALUES (?1, ?2) ON CONFLICT (key) 
            DO UPDATE SET value = excluded.value;
            "#,
        &[
            &format!("{}/port", container_id.clone()),
            &port.clone().unwrap_or("8080".to_owned()),
        ],
    )
    .unwrap();

    let config = Config {
        image: Some(image_name.to_owned()),
        // set to port 80
        exposed_ports: Some(
            [("8080/tcp".to_string(), HashMap::new())]
                .iter()
                .cloned()
                .collect::<HashMap<_, _>>(),
        ),
        // set to port 80
        host_config: Some(HostConfig {
            port_bindings: Some(
                [(
                    "8080/tcp".to_string(),
                    Some(vec![PortBinding {
                        host_ip: Some("".to_owned()),
                        host_port: port,
                    }]),
                )]
                .iter()
                .cloned()
                .collect::<HashMap<_, _>>(),
            ),
            ..Default::default()
        }),
        env: Some(vec![format!(
            "OPENAI_API_KEY={}",
            env::var("OPENAI_API_KEY")
                .unwrap_or("".to_string())
                .as_str(),
        )]), // Set the PORT environment variable to 80

        ..Default::default()
    };

    match data.docker.create_container(Some(options), config).await {
        Ok(_container) => {
            // println!("Created container: {:?}", container);
            return format!(
                "Created container {} from image {}",
                container_id, image_name
            );
        }
        Err(e) => return format!("Error creating container {}: {}", container_id, e),
    };
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
