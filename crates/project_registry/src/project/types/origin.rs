use {
    once_cell::sync::Lazy,
    regex::Regex,
    std::{fmt::Display, iter::zip},
    thiserror::Error as ThisError,
};

/// Simplified URL parser regex. Extracts only the scheme (optional), hostname
/// and port (optional).
static ORIGIN_PARSER_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(([^:]+)://)?([^:/]+)(:([\d]+))?").unwrap());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin<'a> {
    scheme: Option<&'a str>,
    hostname: &'a str,
    hostname_parts: Vec<&'a str>,
    port: Option<u16>,
}

impl<'a> Origin<'a> {
    pub fn matches(&self, other: &Origin) -> bool {
        // if no scheme is specified allow all schemes
        // if a scheme is specified they have to match
        if self.scheme.is_some() && other.scheme.is_some() && self.scheme != other.scheme {
            return false;
        }

        if self.port != other.port {
            return false;
        }

        if self.hostname_parts.len() != other.hostname_parts.len() {
            return false;
        }

        for (&this, &other) in zip(&self.hostname_parts, &other.hostname_parts) {
            if this == "*" {
                continue;
            }

            if this != other {
                return false;
            }
        }

        true
    }

    pub fn hostname(&self) -> &str {
        self.hostname
    }
}

#[derive(Debug, ThisError, PartialEq, Eq)]
pub enum OriginParseError {
    #[error("invalid origin format")]
    InvalidFormat,
    #[error("invalid port number")]
    InvalidPortNumber,
}

impl<'a> TryFrom<&'a str> for Origin<'a> {
    type Error = OriginParseError;

    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        let caps = ORIGIN_PARSER_REGEX
            .captures(s)
            .ok_or(OriginParseError::InvalidFormat)?;

        let scheme = caps.get(2).map(|m| m.as_str());

        let hostname = caps
            .get(3)
            .map(|m| m.as_str())
            .ok_or(OriginParseError::InvalidFormat)?;

        let hostname_parts = hostname.split('.').collect();

        let port = caps
            .get(5)
            .map(|m| m.as_str().parse())
            .transpose()
            .map_err(|_| OriginParseError::InvalidPortNumber)?;

        Ok(Origin {
            scheme,
            hostname,
            hostname_parts,
            port,
        })
    }
}

impl<'a> Display for Origin<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(scheme) = &self.scheme {
            write!(f, "{scheme}://")?;
        }

        let mut host_iter = self.hostname_parts.iter();
        let mut host_part = host_iter.next();

        while let Some(part) = host_part {
            f.write_str(part)?;

            host_part = host_iter.next();

            if host_part.is_some() {
                f.write_str(".")?;
            }
        }

        if let Some(port) = self.port {
            write!(f, ":{port}")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::{Origin, OriginParseError};

    #[test]
    fn parse_origin() {
        assert_eq!(
            Origin::try_from("domain.name"),
            Ok(Origin {
                scheme: None,
                hostname: "domain.name",
                hostname_parts: vec!["domain", "name"],
                port: None,
            })
        );

        assert_eq!(
            Origin::try_from("domain.name:123"),
            Ok(Origin {
                scheme: None,
                hostname: "domain.name",
                hostname_parts: vec!["domain", "name"],
                port: Some(123),
            })
        );

        assert_eq!(
            Origin::try_from("http://domain.name"),
            Ok(Origin {
                scheme: Some("http"),
                hostname: "domain.name",
                hostname_parts: vec!["domain", "name"],
                port: None,
            })
        );

        assert_eq!(
            Origin::try_from("http://*.domain.name:123"),
            Ok(Origin {
                scheme: Some("http"),
                hostname: "*.domain.name",
                hostname_parts: vec!["*", "domain", "name"],
                port: Some(123),
            })
        );

        assert_eq!(
            Origin::try_from("http://domain.name:99999"),
            Err(OriginParseError::InvalidPortNumber)
        );

        let origin = "http://*.domain.name:123";
        assert_eq!(Origin::try_from(origin).unwrap().to_string(), origin);
    }

    #[test]
    fn origin_matching() {
        let o1 = Origin::try_from("http://a.*.domain.name:123").unwrap();
        let o2 = Origin::try_from("http://a.b.domain.name:123").unwrap();

        assert!(o1.matches(&o2));

        let o1 = Origin::try_from("domain.name").unwrap();
        let o2 = Origin::try_from("domain.name").unwrap();

        assert!(o1.matches(&o2));

        let o1 = Origin::try_from("https://a.b.domain.name").unwrap();
        let o2 = Origin::try_from("http://a.b.domain.name").unwrap();

        assert!(!o1.matches(&o2));

        let o1 = Origin::try_from("domain.name:123").unwrap();
        let o2 = Origin::try_from("domain.name:124").unwrap();

        assert!(!o1.matches(&o2));

        let o1 = Origin::try_from("https://domain.name:123").unwrap();
        let o2 = Origin::try_from("domain.name:124").unwrap();

        assert!(!o1.matches(&o2));

        let o1 = Origin::try_from("https://a.b.domain.name/").unwrap();
        let o2 = Origin::try_from("http://a.b.domain.name").unwrap();

        assert!(!o1.matches(&o2));

        let o1 = Origin::try_from("https://a.b.domain.name/").unwrap();
        let o2 = Origin::try_from("a.b.domain.name").unwrap();

        assert!(o1.matches(&o2));

        let o1 = Origin::try_from("https://react-app.walletconnect.com").unwrap();
        let o2 = Origin::try_from("react-app.walletconnect.com").unwrap();

        assert!(o1.matches(&o2));

        // Allow trailing slash.
        let o1 = Origin::try_from("https://react-app.walletconnect.com/").unwrap();
        let o2 = Origin::try_from("react-app.walletconnect.com").unwrap();

        assert!(o1.matches(&o2));

        // Allow custom schema when it's unspecified.
        let o1 = Origin::try_from("custom-schema://react-app.walletconnect.com/").unwrap();
        let o2 = Origin::try_from("react-app.walletconnect.com").unwrap();

        assert!(o1.matches(&o2));
    }
}
