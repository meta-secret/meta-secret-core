use super::MetaServerDataTransfer;
use meta_secret_core::meta_tests::fixture_util::fixture::FixtureRegistry;
use meta_secret_core::node::api::{
    DataSyncResponse, ReadSyncRequest, ServerTailRequest, SyncRequest,
};
use std::sync::Arc;

fn server_tail_request(
    sender: meta_secret_core::node::common::model::user::common::UserData,
) -> SyncRequest {
    SyncRequest::Read(Box::new(ReadSyncRequest::ServerTail(ServerTailRequest {
        producer_reference: None,
        sender,
    })))
}

#[tokio::test]
async fn concurrent_requests_receive_their_own_responses() {
    let registry = FixtureRegistry::empty();
    let request_a = server_tail_request(registry.state.user_creds.client.user());
    let request_b = server_tail_request(registry.state.user_creds.client_b.user());
    let transport = Arc::new(MetaServerDataTransfer::default());

    let first_transport = transport.clone();
    let first_request = request_a.clone();
    let first = tokio::spawn(async move { first_transport.send_request(first_request).await });

    let second_transport = transport.clone();
    let second_request = request_b.clone();
    let second = tokio::spawn(async move { second_transport.send_request(second_request).await });

    let first_envelope = transport.receive_request().await.unwrap();
    let second_envelope = transport.receive_request().await.unwrap();

    for envelope in [second_envelope, first_envelope] {
        let response = if envelope.request == request_a {
            DataSyncResponse::Error {
                msg: "response-a".to_owned(),
            }
        } else {
            assert_eq!(envelope.request, request_b);
            DataSyncResponse::Error {
                msg: "response-b".to_owned(),
            }
        };
        envelope.response_sender.send_async(response).await.unwrap();
    }

    assert_eq!(
        first.await.unwrap().unwrap(),
        DataSyncResponse::Error {
            msg: "response-a".to_owned()
        }
    );
    assert_eq!(
        second.await.unwrap().unwrap(),
        DataSyncResponse::Error {
            msg: "response-b".to_owned()
        }
    );
}
