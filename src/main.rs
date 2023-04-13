use actix_web::{
    http::Method,
    web::Bytes,
    HttpRequest, {post, web, App, HttpServer, Responder},
};
use bollard::{
    container::*,
    service::{HostConfig, PortBinding},
    Docker,
};
use rusqlite::{params, Connection};
use serde::Deserialize;
use std::{
    collections::HashMap,
    env,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
struct AppState {
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
        "add" => {
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
                "INSERT INTO key_value_store (key, value) VALUES (?1, ?2)",
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
            }
        }
        "start" => {
            match data
                .docker
                .start_container::<&str>(&container_id, None)
                .await
            {
                Ok(container) => {
                    println!("Started container: {:?}", container);

                    // // Revisit caching docker inspect results

                    // // get container IP
                    // let inspect_options = InspectContainerOptions {
                    //     size: false,
                    //     ..Default::default()
                    // };

                    // let inspect_result = data
                    //     .docker
                    //     .inspect_container(&container_id, Some(inspect_options))
                    //     .await;

                    // match inspect_result {
                    //     Ok(container) => {
                    //         let ip = container.network_settings.unwrap().ip_address.unwrap();
                    //     }
                    //     Err(e) => {
                    //         return format!("Error inspecting container {}: {}", container_id, e);
                    //     }
                    // }

                    return format!("Started container {}", container_id);
                }
                Err(e) => return format!("Error starting container {}: {}", container_id, e),
            }
        }
        "stop" => {
            if let Err(e) = data.docker.stop_container(&container_id, None).await {
                return format!("Error stopping container {}: {}", container_id, e);
            }
        }
        "remove" => {
            if let Err(e) = data.docker.remove_container(&container_id, None).await {
                return format!("Error removing container {}: {}", container_id, e);
            }
        }
        "list" => {
            let options = ListContainersOptions::<String> {
                all: true,
                ..Default::default()
            };

            match data.docker.list_containers(Some(options)).await {
                Ok(containers) => {
                    // return full list of containers
                    return serde_json::to_string(&containers)
                        .unwrap_or_else(|_| String::from("Error serializing container IDs"));
                }
                Err(e) => return format!("Error listing containers: {}", e),
            };
        }
        _ => return format!("Unsupported action: {}", action),
    }
    format!("{} for {}", action, container_id)
}

async fn reverse_proxy(
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
