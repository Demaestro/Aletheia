//! Strongly named identifiers for service boundaries.

use std::fmt;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            /// Creates a new identifier from a non-empty string.
            pub fn new(value: impl Into<String>) -> Result<Self, &'static str> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err("identifier cannot be empty");
                }
                Ok(Self(value))
            }

            /// Returns the stable string representation.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

id_type!(DeviceId);
id_type!(IntegrationId);
id_type!(ServiceSessionId);
