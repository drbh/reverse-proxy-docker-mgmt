use crate::AppState;
use actix_web::web;
use bollard::{
    container::*,
    models::{HostConfig, PortBinding},
};
use std::{collections::HashMap, env};

pub async fn start(data: web::Data<AppState>, container_id: &String) -> String {
    match data
        .docker
        .start_container::<&str>(container_id, None)
        .await
    {
        Ok(container) => {
            println!("Started container: {:?}", container);
            format!("Started container {}", container_id)
        }
        Err(e) => format!("Error starting container {}: {}", container_id, e),
    }
}

pub async fn stop(data: web::Data<AppState>, container_id: &String) -> String {
    match data.docker.stop_container(container_id, None).await {
        Ok(container) => {
            println!("Stopped container: {:?}", container);
            format!("Stopped container {}", container_id)
        }
        Err(e) => format!("Error stopping container {}: {}", container_id, e),
    }
}

pub async fn remove_container(data: web::Data<AppState>, container_id: &String) -> String {
    match data.docker.remove_container(container_id, None).await {
        Ok(container) => {
            println!("Removed container: {:?}", container);
            format!("Removed container {}", container_id)
        }
        Err(e) => format!("Error removing container {}: {}", container_id, e),
    }
}

pub async fn list_containers(data: web::Data<AppState>) -> String {
    let options = ListContainersOptions::<String> {
        all: true,
        ..Default::default()
    };
    match data.docker.list_containers(Some(options)).await {
        Ok(containers) => serde_json::to_string(&containers)
            .unwrap_or_else(|_| String::from("Error serializing container IDs")),
        Err(e) => format!("Error listing containers: {}", e),
    }
}

pub async fn add_container(
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
            format!(
                "Created container {} from image {}",
                container_id, image_name
            )
        }
        Err(e) => format!("Error creating container {}: {}", container_id, e),
    }
}
