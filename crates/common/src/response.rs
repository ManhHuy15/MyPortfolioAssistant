use serde::Serialize;

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorItem {
    pub code: String,
    pub message: String,
    pub field: Option<String>,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub message: String,
    pub data: Option<T>,
    pub errors: Option<Vec<ApiErrorItem>>,
    pub trace_id: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T, message: impl Into<String>, trace_id: Option<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            data: Some(data),
            errors: None,
            trace_id,
        }
    }
}

impl ApiResponse<()> {
    pub fn error(
        code: impl Into<String>,
        message: impl Into<String>,
        field: Option<String>,
        trace_id: Option<String>,
    ) -> Self {
        let msg = message.into();
        Self {
            success: false,
            message: msg.clone(),
            data: None,
            errors: Some(vec![ApiErrorItem {
                code: code.into(),
                message: msg,
                field,
            }]),
            trace_id,
        }
    }
}
