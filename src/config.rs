use std::collections::HashMap;

use serde::Deserialize;

/// Configuration file structure
#[derive(Debug, Default, Deserialize)]
pub struct Config {
    /// List of repositories to ignore
    pub ignored_repositories: Option<Vec<String>>,

    /// Email address and user name map to normalize the author name
    pub author_map: Option<HashMap<String, String>>,
}
