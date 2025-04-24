// tests/config_grpc.rs
//
// Integration tests for the gRPC config service.
//
// These tests start a dummy gRPC server (`DummyConfigServer`) and verify that
// the client can perform `get_config` and `set_config` operations correctly.
// The server is shared across tests using `OnceCell` to avoid multiple bindings to the same port.

use shared::config::{
    config_service_server::{ConfigService, ConfigServiceServer},
    config_service_client::ConfigServiceClient,
    ConfigUpdate, GetConfigRequest, GetConfigResponse, SetConfigRequest, SetConfigResponse,
    ScannerConfig,
};
use tonic::{transport::Server, Request, Response, Status};
use prost::Message;
use tokio::sync::OnceCell;

#[derive(Default)]
pub struct DummyConfigServer;

#[tonic::async_trait]
impl ConfigService for DummyConfigServer {
    async fn get_config(
        &self,
        _request: Request<GetConfigRequest>,
    ) -> Result<Response<GetConfigResponse>, Status> {
        println!("[Server] Received GetConfig request");
        Ok(Response::new(GetConfigResponse {
            scanner: Some(ScannerConfig {
                enabled: true,
                interval_seconds: 120,
                recursive: false,
                file_extensions: ".exe,.dll".to_string(),
                paths: vec!["C:\\Temp".to_string()],
            }),
            ..Default::default()
        }))
    }

    async fn set_config(
        &self,
        request: Request<SetConfigRequest>,
    ) -> Result<Response<SetConfigResponse>, Status> {
        println!("[Server] Received SetConfig request: {:?}", request);
        let config = request.into_inner().config.unwrap();
        Ok(Response::new(SetConfigResponse {
            success: config.scanner.is_some(),
            message: "OK".to_string(),
        }))
    }
}

static START_SERVER: OnceCell<()> = OnceCell::const_new();

async fn start_grpc_server() {
    START_SERVER
        .get_or_init(|| async {
            println!("[Test Setup] Starting gRPC server on [::1]:50051...");
            tokio::spawn(async {
                Server::builder()
                    .add_service(ConfigServiceServer::new(DummyConfigServer::default()))
                    .serve("[::1]:50051".parse().unwrap())
                    .await
                    .unwrap();
            });
        })
        .await;
}

#[tokio::test]
async fn test_grpc_get_config() {
    println!("[Test] test_grpc_get_config");
    start_grpc_server().await;

    let mut client = ConfigServiceClient::connect("http://[::1]:50051")
        .await
        .expect("connect failed");

    println!("[Client] Sending GetConfig request...");
    let response = client
        .get_config(GetConfigRequest {})
        .await
        .expect("grpc call failed");

    let scanner = response.into_inner().scanner.unwrap();
    println!("[Client] Got scanner config: {:?}", scanner);

    assert_eq!(scanner.interval_seconds, 120);
    assert_eq!(scanner.paths[0], "C:\\Temp");
}

#[tokio::test]
async fn test_grpc_set_config() {
    println!("[Test] test_grpc_set_config");
    start_grpc_server().await;

    let mut client = ConfigServiceClient::connect("http://[::1]:50051")
        .await
        .expect("connect failed");

    let req = SetConfigRequest {
        config: Some(ConfigUpdate {
            scanner: Some(ScannerConfig {
                enabled: true,
                interval_seconds: 300,
                recursive: true,
                file_extensions: ".ps1".into(),
                paths: vec!["C:\\Scripts".into()],
            }),
            ..Default::default()
        }),
    };

    println!("[Client] Sending SetConfig request: {:?}", req);
    let response = client.set_config(req).await.expect("grpc call failed");
    let resp = response.into_inner();
    println!("[Client] Got SetConfig response: {:?}", resp);

    assert!(resp.success);
    assert_eq!(resp.message, "OK");
}
