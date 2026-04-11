use anyhow::{Result, anyhow};
use serde_json::Value;

pub fn effective_project_domain_from_response(response: &Value) -> Result<String> {
    let payload = response
        .get("data")
        .ok_or_else(|| anyhow!("project domains response is missing `data`"))?;

    if let Some(host) = payload
        .get("custom_subdomains")
        .and_then(Value::as_array)
        .and_then(|records| records.first())
        .and_then(|record| record.get("host"))
        .and_then(Value::as_str)
    {
        return Ok(normalize_domain(host));
    }

    let host = payload
        .get("default_domain")
        .and_then(|value| value.get("host"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("project domains response is missing `default_domain.host`"))?;
    Ok(normalize_domain(host))
}

pub fn normalize_domain(value: &str) -> String {
    value.trim().trim_matches('.').to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::effective_project_domain_from_response;

    #[test]
    fn prefers_custom_project_domain_when_present() {
        let domain = effective_project_domain_from_response(&json!({
            "data": {
                "default_domain": { "host": "prj-123.transairobot.com" },
                "custom_subdomains": [
                    { "host": "foo.transairobot.com" }
                ]
            }
        }))
        .unwrap();

        assert_eq!(domain, "foo.transairobot.com");
    }

    #[test]
    fn falls_back_to_default_project_domain() {
        let domain = effective_project_domain_from_response(&json!({
            "data": {
                "default_domain": { "host": "prj-123.transairobot.com" },
                "custom_subdomains": []
            }
        }))
        .unwrap();

        assert_eq!(domain, "prj-123.transairobot.com");
    }
}
