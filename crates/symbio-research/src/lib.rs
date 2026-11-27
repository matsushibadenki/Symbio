//! Bounded provider tool-calling loop. All mathematical tools execute in Rust.
pub mod scratch;
use reqwest::{Client, Url, redirect::Policy};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

pub const MAX_PROMPT: usize = 32 * 1024;
const MAX_RESPONSE: usize = 1024 * 1024;
const MAX_ROUNDS: usize = 5;
const MAX_CALLS: usize = 8;

type Result<T> = std::result::Result<T, String>;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Openai,
    Gemini,
    Local,
}

#[derive(Serialize)]
pub struct Evidence {
    tool: String,
    arguments: Value,
    result: Value,
}

#[derive(Serialize)]
pub struct Report {
    answer: String,
    evidence: Vec<Evidence>,
}

fn endpoint(provider: &Provider, local_base: Option<&str>) -> Result<Url> {
    match provider {
        Provider::Openai => Ok(Url::parse("https://api.openai.com/v1/chat/completions").unwrap()),
        Provider::Gemini => Ok(Url::parse(
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
        )
        .unwrap()),
        Provider::Local => {
            let mut url = Url::parse(local_base.unwrap_or("http://127.0.0.1:11434/v1"))
                .map_err(|_| "invalid_local_url")?;
            let loopback = url.host_str().is_some_and(|host| {
                host == "localhost"
                    || host
                        .trim_matches(['[', ']'])
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
            });
            if !loopback
                || !matches!(url.scheme(), "http" | "https")
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err("invalid_local_url".into());
            }
            url.path_segments_mut()
                .map_err(|_| "invalid_local_url")?
                .pop_if_empty()
                .extend(["chat", "completions"]);
            Ok(url)
        }
    }
}

pub async fn run(
    provider: Provider,
    model: &str,
    prompt: &str,
    local_base: Option<&str>,
    api_key: Option<&str>,
) -> Result<Report> {
    if prompt.trim().is_empty() || prompt.len() > MAX_PROMPT {
        return Err("invalid_research_prompt".into());
    }
    if model.trim().is_empty() || model.len() > 200 || model.chars().any(char::is_control) {
        return Err("invalid_research_model".into());
    }
    let url = endpoint(&provider, local_base)?;
    let key_env = match provider {
        Provider::Openai => "OPENAI_API_KEY",
        Provider::Gemini => "GEMINI_API_KEY",
        Provider::Local => "SYMBIO_LOCAL_API_KEY",
    };
    let inherited = std::env::var(key_env).ok();
    let key = api_key
        .filter(|key| !key.trim().is_empty())
        .or(inherited.as_deref())
        .filter(|key| !key.trim().is_empty());
    if key.is_none() && !matches!(provider, Provider::Local) {
        return Err("missing_research_key".into());
    }

    let mut client = Client::builder()
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(45));
    if matches!(provider, Provider::Local) {
        client = client
            .no_proxy()
            .resolve("localhost", std::net::SocketAddr::from(([127, 0, 0, 1], 0)));
    }
    let client = client.build().map_err(|_| "research_network_error")?;
    let schema = scratch::schema().map_err(|_| "internal_error")?;
    let tools: Vec<_> = schema["tools"]
        .as_array()
        .ok_or("internal_error")?
        .iter()
        .map(|tool| json!({"type":"function","function":{
            "name":tool["name"],"description":tool["description"],"parameters":tool["inputSchema"]
        }}))
        .collect();
    let mut messages = vec![
        json!({"role":"system","content":"Use Symbio tools to check mathematical calculations. Keep model hypotheses distinct from kernel results, cite assumptions and preserve errors or uncertainty. Answer in the user's language."}),
        json!({"role":"user","content":prompt}),
    ];
    let mut evidence = Vec::new();
    for round in 0..MAX_ROUNDS {
        let mut body = json!({"model":model,"messages":messages});
        if matches!(provider, Provider::Openai) {
            body["store"] = json!(false);
        }
        // The final request gives the model room to summarize the evidence.
        if round + 1 < MAX_ROUNDS && evidence.len() < MAX_CALLS {
            body["tools"] = json!(tools);
        }
        let mut request = client.post(url.clone()).json(&body);
        if let Some(key) = key {
            request = request.bearer_auth(key);
        }
        let mut response = request.send().await.map_err(|_| "research_network_error")?;
        if !response.status().is_success() {
            return Err(format!("research_http:{}", response.status().as_u16()));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "research_network_error")?
        {
            if chunk.len() > MAX_RESPONSE - bytes.len() {
                return Err("research_response_limit".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let reply: Value =
            serde_json::from_slice(&bytes).map_err(|_| "invalid_research_response")?;
        let message = &reply["choices"][0]["message"];
        let calls = message["tool_calls"]
            .as_array()
            .filter(|calls| !calls.is_empty());
        let Some(calls) = calls else {
            return message["content"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .map(|answer| Report {
                    answer: answer.into(),
                    evidence,
                })
                .ok_or_else(|| "invalid_research_response".into());
        };
        if round + 1 == MAX_ROUNDS || evidence.len() + calls.len() > MAX_CALLS {
            return Err("research_tool_limit".into());
        }
        // Keep provider-specific tool call metadata, including Gemini thought signatures.
        messages.push(json!({"role":"assistant","content":message["content"],"tool_calls":calls}));
        for call in calls {
            let id = call["id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or("invalid_research_response")?;
            let name = call["function"]["name"]
                .as_str()
                .ok_or("invalid_research_response")?;
            let arguments: Value = call["function"]["arguments"]
                .as_str()
                .and_then(|args| serde_json::from_str(args).ok())
                .unwrap_or(Value::Null);
            let tool = name.to_owned();
            let input = arguments.clone();
            let result = tokio::task::spawn_blocking(move || scratch::execute(&tool, input))
                .await
                .map_err(|_| "internal_error")?
                .unwrap_or_else(|_| json!({"ok":false,"error":{"code":"invalid_tool_call"}}));
            let feedback = json!({"ok":result["ok"] == true && result["cell"]["calculation_error"].is_null(),
                "engine":result["engine"],
                "result_text":result["cell"]["result_text"],"output":result["cell"]["output"],
                "calculation_error":result["cell"]["calculation_error"],
                "preview":result["preview"]["expression"],"error":result["error"]});
            messages.push(json!({"role":"tool","tool_call_id":id,"content":feedback.to_string()}));
            evidence.push(Evidence {
                tool: name.into(),
                arguments,
                result,
            });
        }
    }
    Err("research_tool_limit".into())
}
