use worker::{Response, Result};

pub const OPENAPI_SPEC_JSON: &str = include_str!("../../../../../docs/openapi.json");

const SWAGGER_UI_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>MyPortfolioAssistant — Swagger UI</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" />
  <style>
    body { margin: 0; background: #fafafa; }
  </style>
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
  <script>
    window.onload = () => {
      window.ui = SwaggerUIBundle({
        url: '/api/openapi.json',
        dom_id: '#swagger-ui',
        deepLinking: true,
        persistAuthorization: true
      });
    };
  </script>
</body>
</html>
"#;

pub fn handle_openapi_spec() -> Result<Response> {
    let mut resp = Response::ok(OPENAPI_SPEC_JSON)?;
    resp.headers_mut()
        .set("Content-Type", "application/json; charset=utf-8")?;
    Ok(resp)
}

pub fn handle_swagger_ui() -> Result<Response> {
    let mut resp = Response::from_html(SWAGGER_UI_HTML)?;
    resp.headers_mut()
        .set("Cache-Control", "public, max-age=3600")?;
    Ok(resp)
}
