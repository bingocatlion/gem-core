use super::{target::AlienHttpMethod, AlienError, AlienProvider, AlienTarget};
use primitives::Chain;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt::{Debug, Display},
    sync::Arc,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    pub params: Vec<serde_json::Value>,
}

pub trait JsonRpcRequestConvert {
    fn to_req(&self, id: u64) -> JsonRpcRequest;
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: &str, params: Vec<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: method.into(),
            params,
        }
    }

    pub fn to_target(&self, url: &str) -> Result<AlienTarget, AlienError> {
        let headers = HashMap::from([("Content-Type".into(), "application/json".into())]);
        let body = serde_json::to_vec(self).map_err(|e| AlienError::RequestError {
            msg: format!("Failed to serialize RPC request: {}", e),
        })?;
        Ok(AlienTarget {
            url: url.into(),
            method: AlienHttpMethod::Post,
            headers: Some(headers),
            body: Some(body),
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

impl Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl From<AlienError> for JsonRpcError {
    fn from(err: AlienError) -> Self {
        Self {
            code: -1,
            message: err.to_string(),
        }
    }
}

impl From<JsonRpcError> for AlienError {
    fn from(err: JsonRpcError) -> Self {
        Self::ResponseError { msg: err.message }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcResponse<T> {
    pub id: u64,
    pub result: T,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcErrorResponse {
    pub id: u64,
    pub error: JsonRpcError,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum JsonRpcResult<T> {
    Value(JsonRpcResponse<T>),
    Error(JsonRpcErrorResponse),
}

impl<T> JsonRpcResult<T>
where
    T: Clone,
{
    pub fn take(&self) -> Result<T, JsonRpcError> {
        match self {
            JsonRpcResult::Value(value) => Ok(value.result.clone()),
            JsonRpcResult::Error(error) => Err(error.error.clone()),
        }
    }
}

pub fn batch_into_target<T>(requests: &T, endpoint: &str) -> AlienTarget
where
    T: ?Sized + Serialize,
{
    let headers = HashMap::from([("Content-Type".into(), "application/json".into())]);
    let bytes = serde_json::to_vec(requests).unwrap();
    AlienTarget {
        url: endpoint.into(),
        method: AlienHttpMethod::Post,
        headers: Some(headers),
        body: Some(bytes),
    }
}

#[derive(Debug)]
pub struct JsonRpcClient {
    provider: Arc<dyn AlienProvider>,
    endpoint: String,
}

impl JsonRpcClient {
    pub fn new(provider: Arc<dyn AlienProvider>, endpoint: String) -> Self {
        Self { provider, endpoint }
    }

    pub fn new_with_chain(provider: Arc<dyn AlienProvider>, chain: Chain) -> Self {
        let endpoint = provider.get_endpoint(chain).unwrap();
        Self::new(provider, endpoint)
    }

    pub async fn call<T, U>(&self, call: &T) -> Result<JsonRpcResult<U>, JsonRpcError>
    where
        T: JsonRpcRequestConvert,
        U: DeserializeOwned,
    {
        self.call_with_cache(call, None).await
    }

    pub async fn call_method_with_param<T, U>(&self, method: &str, params: T, ttl: Option<u64>) -> Result<JsonRpcResult<U>, AlienError>
    where
        T: Serialize,
        U: DeserializeOwned,
    {
        let params_value = serde_json::to_value(params).map_err(|e| AlienError::RequestError {
            msg: format!("Failed to serialize RPC params: {}", e),
        })?;

        // Wrap single object/value in an array if it's not already an array
        let params_array = match params_value {
            serde_json::Value::Array(arr) => arr,
            _ => vec![params_value],
        };

        let request = JsonRpcRequest::new(1, method, params_array);
        let mut target = request.to_target(&self.endpoint)?;
        if let Some(ttl) = ttl {
            target = target.set_cache_ttl(ttl);
        }
        let response_data = self.provider.request(target).await?;

        // Deserialize into the JsonRpcResult enum first
        let rpc_result: JsonRpcResult<U> = serde_json::from_slice(&response_data).map_err(|e| AlienError::ResponseError {
            msg: format!("Failed to parse JSON-RPC response: {}", e),
        })?;

        Ok(rpc_result)
    }

    pub async fn call_with_cache<T, U>(&self, call: &T, ttl: Option<u64>) -> Result<JsonRpcResult<U>, JsonRpcError>
    where
        T: JsonRpcRequestConvert,
        U: DeserializeOwned,
    {
        let request = call.to_req(1);
        let mut target = batch_into_target(&request, &self.endpoint);
        if let Some(ttl) = ttl {
            target = target.set_cache_ttl(ttl);
        }
        let data = self.provider.request(target).await?;
        let result: JsonRpcResult<U> = serde_json::from_slice(&data).map_err(|err| AlienError::ResponseError { msg: err.to_string() })?;
        Ok(result)
    }

    pub async fn batch_call<T, U>(&self, calls: Vec<T>) -> Result<Vec<JsonRpcResult<U>>, AlienError>
    where
        T: JsonRpcRequestConvert,
        U: DeserializeOwned,
    {
        let requests: Vec<JsonRpcRequest> = calls.iter().enumerate().map(|(index, request)| request.to_req(index as u64 + 1)).collect();

        let targets = vec![batch_into_target(&requests, &self.endpoint)];

        let data_array = self.provider.batch_request(targets).await?;
        let data = data_array.first().ok_or(AlienError::ResponseError { msg: "No result".into() })?;

        let results: Vec<JsonRpcResult<U>> = serde_json::from_slice(data).map_err(|err| AlienError::ResponseError { msg: err.to_string() })?;
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use core::panic;

    use super::*;

    #[test]
    fn test_batch_into_target() {
        let requests = vec![
            JsonRpcRequest::new(1, "eth_gasPrice", vec![]),
            JsonRpcRequest::new(2, "eth_blockNumber", vec!["0x95222290DD7278Aa3Ddd389Cc1E1d165CC4BAfe5".into(), "latest".into()]),
            JsonRpcRequest::new(3, "eth_chainId", vec![]),
        ];
        let endpoint = "http://localhost:8080";
        let target = batch_into_target(&requests, endpoint);

        assert_eq!(target.url, endpoint);
        assert_eq!(target.method, AlienHttpMethod::Post);
        assert_eq!(target.headers.unwrap().get("Content-Type").unwrap(), "application/json");
        assert_eq!(
            String::from_utf8(target.body.unwrap()).unwrap(),
            r#"[{"jsonrpc":"2.0","id":1,"method":"eth_gasPrice","params":[]},{"jsonrpc":"2.0","id":2,"method":"eth_blockNumber","params":["0x95222290DD7278Aa3Ddd389Cc1E1d165CC4BAfe5","latest"]},{"jsonrpc":"2.0","id":3,"method":"eth_chainId","params":[]}]"#
        );
    }

    #[test]
    fn test_decode_json_rpc_error_response() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": 3,
                "message": "execution reverted: revert: toAddress_outOfBounds"
            }
        }"#;
        let result = serde_json::from_str::<JsonRpcResult<String>>(json).unwrap();
        if let JsonRpcResult::Error(value) = result {
            assert_eq!(value.id, 1);
            assert_eq!(value.error.code, 3);
            assert_eq!(value.error.message, "execution reverted: revert: toAddress_outOfBounds");
        } else {
            panic!("unexpected response: {:?}", result);
        }
    }

    #[test]
    fn test_decode_json_rpc_response() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": "0x21e3bb1a6"
        }"#;
        let result = serde_json::from_str::<JsonRpcResult<String>>(json).unwrap();
        if let JsonRpcResult::Value(value) = result {
            assert_eq!(value.id, 1);
            assert_eq!(value.result, "0x21e3bb1a6");
        } else {
            panic!("unexpected response: {:?}", result);
        }
    }
}
