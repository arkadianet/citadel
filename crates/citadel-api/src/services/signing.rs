//! ErgoPay signing flow: transaction reduction and request lifecycle.

use citadel_core::BoxId;
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_node_client::NodeClient;
use ergo_tx::Eip12UnsignedTx;
use ergopay_core::{reduce_transaction, reduce_transaction_fallback};
use ergopay_server::RequestStatus;

use super::error::{IntoServiceError, ServiceResult};
use crate::dto::{MintSignRequest, MintSignResponse, MintTxStatusResponse};
use crate::AppState;

async fn fetch_boxes_by_ids(
    client: &NodeClient,
    box_ids: &[&String],
) -> ServiceResult<Vec<ErgoBox>> {
    let mut boxes = Vec::with_capacity(box_ids.len());
    for box_id in box_ids {
        let ergo_box = client
            .get_box_by_id(&BoxId::new(box_id.as_str()))
            .await
            .into_service()?;
        boxes.push(ergo_box);
    }
    Ok(boxes)
}

pub async fn start_mint_sign(
    state: &AppState,
    request: MintSignRequest,
) -> ServiceResult<MintSignResponse> {
    let client = state.require_node_client().await?;

    let eip12_tx: Eip12UnsignedTx = serde_json::from_value(request.unsigned_tx.clone())
        .map_err(|e| format!("Failed to parse unsigned tx: {}", e))?;

    let input_boxes = fetch_boxes_by_ids(
        &client,
        &eip12_tx
            .inputs
            .iter()
            .map(|i| &i.box_id)
            .collect::<Vec<_>>(),
    )
    .await?;

    let data_input_boxes = fetch_boxes_by_ids(
        &client,
        &eip12_tx
            .data_inputs
            .iter()
            .map(|d| &d.box_id)
            .collect::<Vec<_>>(),
    )
    .await?;

    let reduced_bytes =
        match reduce_transaction(&eip12_tx, input_boxes, data_input_boxes, &client).await {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::warn!("Standard reduction failed ({}), using fallback", e);
                reduce_transaction_fallback(&eip12_tx)
                    .map_err(|e| format!("Fallback reduction also failed: {}", e))?
            }
        };

    let server = state.ergopay_server().await.into_service()?;

    let (request_id, ergopay_url) = server
        .create_tx_request(reduced_bytes, request.unsigned_tx.clone(), request.message)
        .await;

    let nautilus_url = server.get_nautilus_url(&request_id);

    Ok(MintSignResponse {
        request_id,
        ergopay_url,
        nautilus_url,
    })
}

pub async fn get_mint_tx_status(
    state: &AppState,
    request_id: &str,
) -> ServiceResult<MintTxStatusResponse> {
    let server = state.ergopay_server().await.into_service()?;

    match server.get_request_status(request_id).await {
        Some(RequestStatus::Pending) => Ok(MintTxStatusResponse {
            status: "pending".to_string(),
            tx_id: None,
            error: None,
        }),
        Some(RequestStatus::TxSubmitted { tx_id }) => Ok(MintTxStatusResponse {
            status: "submitted".to_string(),
            tx_id: Some(tx_id),
            error: None,
        }),
        Some(RequestStatus::AddressReceived { .. }) => Ok(MintTxStatusResponse {
            status: "pending".to_string(),
            tx_id: None,
            error: None,
        }),
        Some(RequestStatus::Expired) => Ok(MintTxStatusResponse {
            status: "expired".to_string(),
            tx_id: None,
            error: Some("Request expired".to_string()),
        }),
        Some(RequestStatus::Signed { .. }) => Ok(MintTxStatusResponse {
            status: "signed".to_string(),
            tx_id: None,
            error: None,
        }),
        Some(RequestStatus::Failed(msg)) => Ok(MintTxStatusResponse {
            status: "failed".to_string(),
            tx_id: None,
            error: Some(msg),
        }),
        None => Ok(MintTxStatusResponse {
            status: "unknown".to_string(),
            tx_id: None,
            error: Some("Request not found".to_string()),
        }),
    }
}
