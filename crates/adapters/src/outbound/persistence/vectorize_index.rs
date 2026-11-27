use async_trait::async_trait;
use domain::errors::DomainError;
use domain::ports::outbound::{VectorIndexPort, VectorMatch, VectorRecord};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, JsValue, prelude::*};
use wasm_bindgen_futures::JsFuture;
use worker::Env;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = js_sys::Object)]
    #[derive(Debug, Clone)]
    type VectorizeIndexSys;

    #[wasm_bindgen(structural, method, js_name = upsert)]
    fn upsert(this: &VectorizeIndexSys, vectors: JsValue) -> js_sys::Promise;

    #[wasm_bindgen(structural, method, js_name = query)]
    fn query(this: &VectorizeIndexSys, vector: JsValue, options: JsValue) -> js_sys::Promise;
}

pub struct VectorizeIndexAdapter {
    inner: VectorizeIndexSys,
}

impl VectorizeIndexAdapter {
    pub fn from_env(env: &Env, binding: &str) -> Result<Self, DomainError> {
        let val = js_sys::Reflect::get(env, &JsValue::from_str(binding)).map_err(|_| {
            DomainError::Storage(format!(
                "Env does not contain Vectorize binding `{binding}`"
            ))
        })?;
        if val.is_undefined() || val.is_null() {
            return Err(DomainError::Storage(format!(
                "Vectorize binding `{binding}` is undefined"
            )));
        }
        Ok(Self {
            inner: val.unchecked_into(),
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VectorizeMetadata<'a> {
    doc_title: &'a str,
    category: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VectorizeUpsertItem<'a> {
    id: &'a str,
    values: &'a [f32],
    metadata: VectorizeMetadata<'a>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct VectorizeQueryOptions {
    top_k: usize,
}

#[derive(Debug, Deserialize)]
struct VectorizeMatchItem {
    id: String,
    score: f32,
}

#[derive(Debug, Deserialize)]
struct VectorizeQueryResponse {
    #[serde(default)]
    matches: Vec<VectorizeMatchItem>,
}

#[async_trait(?Send)]
impl VectorIndexPort for VectorizeIndexAdapter {
    async fn upsert_vectors_batch(&self, records: &[VectorRecord]) -> Result<(), DomainError> {
        if records.is_empty() {
            return Ok(());
        }

        let items: Vec<VectorizeUpsertItem<'_>> = records
            .iter()
            .map(|r| VectorizeUpsertItem {
                id: &r.id,
                values: &r.values,
                metadata: VectorizeMetadata {
                    doc_title: &r.doc_title,
                    category: &r.category,
                },
            })
            .collect();

        let js_vectors = items
            .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        let promise = self.inner.upsert(js_vectors);
        JsFuture::from(promise)
            .await
            .map_err(|e| DomainError::Storage(format!("Vectorize upsert error: {e:?}")))?;

        Ok(())
    }

    async fn query_top_k(
        &self,
        vector: &[f32],
        top_k: usize,
    ) -> Result<Vec<VectorMatch>, DomainError> {
        let js_vector = vector
            .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        let opts = VectorizeQueryOptions { top_k };
        let js_opts = opts
            .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        let promise = self.inner.query(js_vector, js_opts);
        let js_res = JsFuture::from(promise)
            .await
            .map_err(|e| DomainError::Storage(format!("Vectorize query error: {e:?}")))?;

        let parsed: VectorizeQueryResponse = serde_wasm_bindgen::from_value(js_res)
            .map_err(|e| DomainError::Storage(e.to_string()))?;

        Ok(parsed
            .matches
            .into_iter()
            .map(|m| VectorMatch {
                id: m.id,
                score: m.score,
            })
            .collect())
    }
}
