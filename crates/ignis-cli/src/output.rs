use std::fmt;

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Warning {
    pub code: &'static str,
    pub message: String,
}

impl Warning {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Drift {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
}

impl Drift {
    pub fn for_service(
        code: &'static str,
        service: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            service: Some(service.into()),
        }
    }
}

#[derive(Debug)]
pub struct CliError {
    code: &'static str,
    message: String,
    warnings: Vec<Warning>,
    drift: Vec<Drift>,
    details: Vec<String>,
}

impl CliError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            code: "command_failed",
            message: message.into(),
            warnings: Vec::new(),
            drift: Vec::new(),
            details: Vec::new(),
        }
    }

    pub fn code(mut self, code: &'static str) -> Self {
        self.code = code;
        self
    }

    pub fn with_warnings(mut self, warnings: Vec<Warning>) -> Self {
        self.warnings = warnings;
        self
    }

    pub fn with_details<I, S>(mut self, details: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.details = details.into_iter().map(Into::into).collect();
        self
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

#[derive(Debug, Serialize)]
struct Envelope {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<Warning>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    drift: Vec<Drift>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ErrorEnvelope>,
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    code: String,
    message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    details: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    causes: Vec<String>,
}

pub fn success<T>(data: T) -> Result<()>
where
    T: Serialize,
{
    success_with(data, Vec::new(), Vec::new())
}

pub fn success_with<T>(data: T, warnings: Vec<Warning>, drift: Vec<Drift>) -> Result<()>
where
    T: Serialize,
{
    print_envelope(Envelope {
        ok: true,
        data: Some(serde_json::to_value(data)?),
        warnings,
        drift,
        error: None,
    })
}

pub fn failure(error: &anyhow::Error) -> Result<()> {
    if let Some(cli_error) = error.downcast_ref::<CliError>() {
        return print_envelope(Envelope {
            ok: false,
            data: None,
            warnings: cli_error.warnings.clone(),
            drift: cli_error.drift.clone(),
            error: Some(ErrorEnvelope {
                code: cli_error.code.to_owned(),
                message: cli_error.message.clone(),
                details: cli_error.details.clone(),
                causes: error
                    .chain()
                    .skip(1)
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            }),
        });
    }

    print_envelope(Envelope {
        ok: false,
        data: None,
        warnings: Vec::new(),
        drift: Vec::new(),
        error: Some(ErrorEnvelope {
            code: "command_failed".to_owned(),
            message: error.to_string(),
            details: Vec::new(),
            causes: error
                .chain()
                .skip(1)
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        }),
    })
}

fn print_envelope(envelope: Envelope) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&envelope)?);
    Ok(())
}
