use crate::AppState;
use actix_web::web;
use bollard::container::*;

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
