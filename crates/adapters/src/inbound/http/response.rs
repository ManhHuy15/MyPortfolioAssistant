use common::response::ApiResponse;
use serde::Serialize;
use worker::{Response, Result};

pub fn json_response<T: Serialize>(status: u16, body: &ApiResponse<T>) -> Result<Response> {
    Ok(Response::from_json(body)?.with_status(status))
}
