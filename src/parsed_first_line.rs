use crate::{HttpVerbs, ResponseableError};

#[derive(Debug, PartialEq, Eq)]
pub struct ParsedFirstLine {
    pub method: HttpVerbs,
    pub path: String,
    pub query: Option<String>,
    pub version: String,
}

impl ParsedFirstLine {
    pub fn parse(line: String) -> Result<Self, ResponseableError> {
        let splitted: Vec<&str> = line.split(' ').collect();
        if splitted.len() != 3 {
            return Err(ResponseableError::NotHttpConform);
        }
        let path_query = splitted[1].split_once('?');
        let method = HttpVerbs::map_method(splitted[0])?;
        if let Some((path, query)) = path_query {
            Ok(Self {
                method,
                path: path.to_string(),
                query: Some(query.to_string()),
                version: splitted[2].to_string(),
            })
        } else {
            Ok(Self {
                method,
                path: splitted[1].to_string(),
                query: None,
                version: splitted[2].to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_version() {
        assert_eq!(
            ParsedFirstLine::parse("GET /path".to_string()),
            Err(ResponseableError::NotHttpConform)
        );
    }

    #[test]
    fn invalid_verb() {
        assert_eq!(
            ParsedFirstLine::parse("BLUB /path HTTP/1.1".to_string()),
            Err(ResponseableError::MethodNotImplemented("BLUB".to_string()))
        );
    }

    #[test]
    fn no_query() {
        assert_eq!(
            ParsedFirstLine::parse("GET /path HTTP/1.1".to_string()),
            Ok(ParsedFirstLine {
                method: HttpVerbs::GET,
                path: "/path".to_string(),
                query: None,
                version: "HTTP/1.1".to_string()
            })
        );
    }

    #[test]
    fn with_query() {
        assert_eq!(
            ParsedFirstLine::parse("GET /path?blub&foo=bar HTTP/1.1".to_string()),
            Ok(ParsedFirstLine {
                method: HttpVerbs::GET,
                path: "/path".to_string(),
                query: Some("blub&foo=bar".to_string()),
                version: "HTTP/1.1".to_string()
            })
        );
    }
}
