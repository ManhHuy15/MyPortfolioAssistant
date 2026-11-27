use application::dtos::HealthResponseDto;
use common::response::ApiResponse;
use worker::{Request, Response, Result};

use super::response::json_response;

pub fn handle_health(req: &Request) -> Result<Response> {
    let trace_id = req.headers().get("cf-ray").ok().flatten();
    let envelope = ApiResponse::success(
        HealthResponseDto {
            status: "healthy".to_string(),
            service: "my-portfolio-assistant".to_string(),
        },
        "OK",
        trace_id,
    );
    json_response(200, &envelope)
}
