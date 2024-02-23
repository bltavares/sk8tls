use std::collections::HashMap;

#[derive(serde::Deserialize, Debug, PartialEq, Eq)]
pub struct SuccessParsing {}

#[derive(serde::Deserialize, Debug, PartialEq, Eq)]
pub struct FailedParsingDetails {
    pub causes: Vec<FailedParsingCause>,
}

#[derive(serde::Deserialize, Debug, PartialEq, Eq)]
pub struct FailedFieldReference {
    pub field: String,
    pub reason: String,
}

#[derive(serde::Deserialize, Debug, PartialEq, Eq)]
pub struct FailedParsingCause {
    pub message: String,
    #[serde(flatten)]
    pub field: Option<FailedFieldReference>,
}

#[derive(serde::Deserialize, Debug, PartialEq, Eq)]
pub struct FailedParsing {
    pub code: u32,
    pub message: String,
    pub reason: String,
    pub details: FailedParsingDetails,
}

pub type Filename = String;

#[derive(serde::Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "status")]
pub enum DocumentResult {
    Success(SuccessParsing),
    Failure(FailedParsing),
}

pub type KubectlValidateResponse = HashMap<Filename, Vec<DocumentResult>>;

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use simd_json as json;

    use super::*;

    #[test]
    fn test_kubectl_validate_response_parse() {
        let mut payload: Vec<_> = r#"
        {
            "deployment.yaml": [
                {
                    "metadata": {},
                    "status": "Success"
                },
                {
                    "metadata": {},
                    "status": "Failure",
                    "message": " \"\" is invalid: spec.a: Invalid value: value provided for unknown field",
                    "reason": "Invalid",
                    "details": {
                        "causes": [
                            {
                                "reason": "FieldValueInvalid",
                                "message": "Invalid value: value provided for unknown field",
                                "field": "spec.a"
                            }
                        ]
                    },
                    "code": 422
                },
                {
                    "metadata": {},
                    "status": "Failure",
                    "message": "Internal error occurred: failed to retrieve validator: failed to locate OpenAPI spec for GV: ",
                    "reason": "InternalError",
                    "details": {
                        "causes": [
                            {
                                "message": "failed to retrieve validator: failed to locate OpenAPI spec for GV: "
                            }
                        ]
                    },
                    "code": 500
                }
            ]
        }
        "#.bytes().collect();

        let expected = {
            use std::collections::HashMap;
            let mut map = HashMap::new();
            map.insert(
                "deployment.yaml".to_string(),
                vec![
                    super::DocumentResult::Success(super::SuccessParsing {}),
                    super::DocumentResult::Failure(super::FailedParsing {
                        code: 422,
                        message: " \"\" is invalid: spec.a: Invalid value: value provided for unknown field".to_string(),
                        reason: "Invalid".to_string(),
                        details: super::FailedParsingDetails {
                            causes: vec![super::FailedParsingCause {
                                message: "Invalid value: value provided for unknown field".to_string(),
                                field: Some(super::FailedFieldReference {
                                    reason: "FieldValueInvalid".to_string(),
                                    field: "spec.a".to_string(),
                                }),
                            }],
                        },
                    }),
                    super::DocumentResult::Failure(super::FailedParsing {
                        code: 500,
                        message: "Internal error occurred: failed to retrieve validator: failed to locate OpenAPI spec for GV: ".to_string(),
                        reason: "InternalError".to_string(),
                        details: super::FailedParsingDetails {
                            causes: vec![super::FailedParsingCause {
                                message: "failed to retrieve validator: failed to locate OpenAPI spec for GV: ".to_string(),
                                field: None,
                            }],
                        },
                    }),
                ],
            );
            map
        };

        assert_eq!(
            expected,
            json::from_slice::<KubectlValidateResponse>(&mut payload).unwrap(),
        );
    }
}
