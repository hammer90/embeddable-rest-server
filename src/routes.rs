use std::collections::HashMap;

#[derive(Debug, PartialEq, Eq)]
enum RouteTyp {
    Fixed(String),
    Param(String),
}

impl From<&str> for RouteTyp {
    fn from(s: &str) -> Self {
        if s.starts_with(':') {
            Self::Param(s.to_string())
        } else {
            Self::Fixed(s.to_string())
        }
    }
}

impl RouteTyp {
    fn search_eq(&self, other: &str) -> bool {
        match self {
            Self::Fixed(fixed) => fixed == other,
            Self::Param(param) => {
                if other.starts_with(':') {
                    param == other
                } else {
                    true
                }
            }
        }
    }

    fn add_eq(&self, other: &str) -> Result<bool, RoutesError> {
        match self {
            Self::Fixed(fixed) => Ok(fixed == other),
            Self::Param(param) => {
                if param == other {
                    Ok(true)
                } else {
                    Err(RoutesError::ParamMismatch(
                        param.to_string(),
                        other.to_string(),
                    ))
                }
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Route<T> {
    key: RouteTyp,
    item: Option<T>,
    childs: Vec<Route<T>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RoutesError {
    RouteExists,
    ParamMismatch(String, String),
}

fn split_head(org: &str) -> (&str, &str) {
    if let Some((head, rest)) = org.split_once('/') {
        (head, rest)
    } else {
        (org, "")
    }
}

impl<T: Copy> Route<T> {
    fn new(path: &str, item: T) -> Self {
        let path = uniform_path(path);
        if let Some((curr, rest)) = path.split_once('/') {
            Self {
                key: curr.into(),
                item: None,
                childs: vec![Route::new(rest, item)],
            }
        } else {
            Self {
                key: path.into(),
                item: Some(item),
                childs: vec![],
            }
        }
    }

    fn find(&self, path: &str) -> Option<(&Route<T>, HashMap<String, String>)> {
        let path = uniform_path(path);
        if let Some((curr, rest)) = path.split_once('/') {
            if self.key.search_eq(curr) {
                for child in &self.childs {
                    let found = child.find(rest);
                    if let Some((found, mut params)) = found {
                        if let RouteTyp::Param(param) = &self.key {
                            params.insert(param[1..].to_string(), curr.to_string());
                        }
                        return Some((found, params));
                    }
                }
            }
        } else if self.key.search_eq(path) {
            let mut params = HashMap::new();
            if let RouteTyp::Param(param) = &self.key {
                params.insert(param[1..].to_string(), path.to_string());
            }
            return Some((self, params));
        }
        None
    }

    fn add(self, path: &str, item: T) -> Result<Route<T>, RoutesError> {
        if path.is_empty() {
            if self.item.is_some() {
                return Err(RoutesError::RouteExists);
            } else {
                return Ok(Route {
                    key: self.key,
                    item: Some(item),
                    childs: self.childs,
                });
            }
        }
        let (curr, rest) = split_head(path);
        let mut new_childs = vec![];
        let mut added = false;
        for child in self.childs {
            if child.key.add_eq(curr)? {
                new_childs.push(child.add(rest, item)?);
                added = true;
            } else {
                new_childs.push(child);
            }
        }
        if !added {
            new_childs.push(Route::new(path, item));
        }
        Ok(Route {
            key: self.key,
            item: self.item,
            childs: new_childs,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Routes<T> {
    root: Route<T>,
}

impl<T: Copy> Routes<T> {
    pub fn new() -> Self {
        Self {
            root: Route {
                key: "$root".into(),
                item: None,
                childs: vec![],
            },
        }
    }

    pub fn add(self, path: &str, item: T) -> Result<Self, RoutesError> {
        let path = uniform_path(path);
        Ok(Self {
            root: self.root.add(path, item)?,
        })
    }

    pub fn find(&self, path: &str) -> Option<(T, HashMap<String, String>)> {
        let path = uniform_path(path);
        let route = self.root.find(format!("$root/{}", path).as_str());
        if let Some(found) = route {
            if let Some(item) = found.0.item {
                return Some((item, found.1));
            }
        }
        None
    }
}

fn uniform_path(path: &str) -> &str {
    if path.is_empty() || path == "/" {
        return "";
    }
    let start = if path.starts_with('/') { 1 } else { 0 };
    let end = if path.ends_with('/') { 1 } else { 0 };
    &path[start..path.len() - end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform() {
        assert_eq!(uniform_path(""), "");
        assert_eq!(uniform_path("/"), "");
        assert_eq!(uniform_path("A"), "A");
        assert_eq!(uniform_path("/B"), "B");
        assert_eq!(uniform_path("C/"), "C");
        assert_eq!(uniform_path("/D/"), "D");
    }

    #[test]
    fn splits() {
        assert_eq!(split_head("A"), ("A", ""));
        assert_eq!(split_head("A/B"), ("A", "B"));
        assert_eq!(split_head("A/B/C"), ("A", "B/C"));
    }

    #[test]
    fn new_root() {
        let route = Route::new("/A", 0);
        assert_eq!(
            route,
            Route {
                key: RouteTyp::Fixed("A".to_string()),
                item: Some(0),
                childs: vec![]
            }
        );
    }

    #[test]
    fn new_child() {
        let route = Route::new("/A/B", 0);
        assert_eq!(
            route,
            Route {
                key: RouteTyp::Fixed("A".to_string()),
                item: None,
                childs: vec![Route {
                    key: RouteTyp::Fixed("B".to_string()),
                    item: Some(0),
                    childs: vec![]
                }]
            }
        );
    }

    #[test]
    fn with_params() {
        let route = Route::new("/A/:B/C", 0);
        assert_eq!(
            route,
            Route {
                key: RouteTyp::Fixed("A".to_string()),
                item: None,
                childs: vec![Route {
                    key: RouteTyp::Param(":B".to_string()),
                    item: None,
                    childs: vec![Route {
                        key: RouteTyp::Fixed("C".to_string()),
                        item: Some(0),
                        childs: vec![]
                    }]
                }]
            }
        );
    }

    #[test]
    fn find() {
        let route = Route::new("/A/B", 0);
        assert_eq!(route.find("/C"), None);
        assert_eq!(
            route.find("/A").unwrap(),
            (
                &Route {
                    key: RouteTyp::Fixed("A".to_string()),
                    item: None,
                    childs: vec![Route {
                        key: RouteTyp::Fixed("B".to_string()),
                        item: Some(0),
                        childs: vec![]
                    }]
                },
                HashMap::new()
            )
        );
        assert_eq!(
            route.find("/A/B").unwrap(),
            (
                &Route {
                    key: RouteTyp::Fixed("B".to_string()),
                    item: Some(0),
                    childs: vec![]
                },
                HashMap::new()
            )
        );
        assert_eq!(route.find("/A/C"), None);
    }

    #[test]
    fn add_first() {
        let routes = Routes::new();
        let routes = routes.add("/A", 0).unwrap();
        assert_eq!(
            routes,
            Routes {
                root: Route {
                    key: RouteTyp::Fixed("$root".to_string()),
                    item: None,
                    childs: vec![Route {
                        key: RouteTyp::Fixed("A".to_string()),
                        item: Some(0),
                        childs: vec![]
                    }]
                }
            }
        )
    }

    #[test]
    fn add_first_deep() {
        let routes = Routes::new();
        let routes = routes.add("/A/B", 0).unwrap();
        assert_eq!(
            routes,
            Routes {
                root: Route {
                    key: RouteTyp::Fixed("$root".to_string()),
                    item: None,
                    childs: vec![Route {
                        key: RouteTyp::Fixed("A".to_string()),
                        item: None,
                        childs: vec![Route {
                            key: RouteTyp::Fixed("B".to_string()),
                            item: Some(0),
                            childs: vec![]
                        }]
                    }]
                }
            }
        )
    }

    #[test]
    fn add_second() {
        let routes = Routes::new();
        let routes = routes.add("/A", 0).unwrap();
        let routes = routes.add("/B", 1).unwrap();
        assert_eq!(
            routes,
            Routes {
                root: Route {
                    key: RouteTyp::Fixed("$root".to_string()),
                    item: None,
                    childs: vec![
                        Route {
                            key: RouteTyp::Fixed("A".to_string()),
                            item: Some(0),
                            childs: vec![]
                        },
                        Route {
                            key: RouteTyp::Fixed("B".to_string()),
                            item: Some(1),
                            childs: vec![]
                        }
                    ]
                }
            }
        )
    }

    #[test]
    fn add_second_duplicate() {
        let routes = Routes::new();
        let routes = routes.add("/A", 0).unwrap();
        let error = routes.add("/A", 1).unwrap_err();
        assert_eq!(error, RoutesError::RouteExists)
    }

    #[test]
    fn add_second_deep() {
        let routes = Routes::new();
        let routes = routes.add("/A/B", 0).unwrap();
        let routes = routes.add("/A/C", 1).unwrap();
        assert_eq!(
            routes,
            Routes {
                root: Route {
                    key: RouteTyp::Fixed("$root".to_string()),
                    item: None,
                    childs: vec![Route {
                        key: RouteTyp::Fixed("A".to_string()),
                        item: None,
                        childs: vec![
                            Route {
                                key: RouteTyp::Fixed("B".to_string()),
                                item: Some(0),
                                childs: vec![]
                            },
                            Route {
                                key: RouteTyp::Fixed("C".to_string()),
                                item: Some(1),
                                childs: vec![]
                            }
                        ]
                    },]
                }
            }
        )
    }

    #[test]
    fn add_second_deep_duplicate() {
        let routes = Routes::new();
        let routes = routes.add("/A/B", 0).unwrap();
        let error = routes.add("/A/B", 1).unwrap_err();
        assert_eq!(error, RoutesError::RouteExists)
    }

    #[test]
    fn add_child() {
        let routes = Routes::new();
        let routes = routes.add("/A/B", 0).unwrap();
        let routes = routes.add("/A/B/C", 1).unwrap();
        assert_eq!(
            routes,
            Routes {
                root: Route {
                    key: RouteTyp::Fixed("$root".to_string()),
                    item: None,
                    childs: vec![Route {
                        key: RouteTyp::Fixed("A".to_string()),
                        item: None,
                        childs: vec![Route {
                            key: RouteTyp::Fixed("B".to_string()),
                            item: Some(0),
                            childs: vec![Route {
                                key: RouteTyp::Fixed("C".to_string()),
                                item: Some(1),
                                childs: vec![]
                            }]
                        }]
                    },]
                }
            }
        )
    }

    #[test]
    fn add_index() {
        let routes = Routes::new();
        let routes = routes.add("/A/B", 0).unwrap();
        let routes = routes.add("/A", 1).unwrap();
        assert_eq!(
            routes,
            Routes {
                root: Route {
                    key: RouteTyp::Fixed("$root".to_string()),
                    item: None,
                    childs: vec![Route {
                        key: RouteTyp::Fixed("A".to_string()),
                        item: Some(1),
                        childs: vec![Route {
                            key: RouteTyp::Fixed("B".to_string()),
                            item: Some(0),
                            childs: vec![]
                        }]
                    },]
                }
            }
        )
    }

    #[test]
    fn add_index_deep() {
        let routes = Routes::new();
        let routes = routes.add("/A/B/C", 0).unwrap();
        let routes = routes.add("/A/B", 1).unwrap();
        assert_eq!(
            routes,
            Routes {
                root: Route {
                    key: RouteTyp::Fixed("$root".to_string()),
                    item: None,
                    childs: vec![Route {
                        key: RouteTyp::Fixed("A".to_string()),
                        item: None,
                        childs: vec![Route {
                            key: RouteTyp::Fixed("B".to_string()),
                            item: Some(1),
                            childs: vec![Route {
                                key: RouteTyp::Fixed("C".to_string()),
                                item: Some(0),
                                childs: vec![]
                            }]
                        }]
                    },]
                }
            }
        )
    }

    #[test]
    fn add_root() {
        let routes = Routes::new();
        let routes = routes.add("/A/B", 0).unwrap();
        let routes = routes.add("/", 1).unwrap();
        assert_eq!(
            routes,
            Routes {
                root: Route {
                    key: RouteTyp::Fixed("$root".to_string()),
                    item: Some(1),
                    childs: vec![Route {
                        key: RouteTyp::Fixed("A".to_string()),
                        item: None,
                        childs: vec![Route {
                            key: RouteTyp::Fixed("B".to_string()),
                            item: Some(0),
                            childs: vec![]
                        }]
                    },]
                }
            }
        )
    }

    #[test]
    fn add_same_param() {
        let routes = Routes::new();
        let routes = routes.add("/:A/B", 0).unwrap();
        let routes = routes.add("/:A/C", 1).unwrap();
        assert_eq!(
            routes,
            Routes {
                root: Route {
                    key: RouteTyp::Fixed("$root".to_string()),
                    item: None,
                    childs: vec![Route {
                        key: RouteTyp::Param(":A".to_string()),
                        item: None,
                        childs: vec![
                            Route {
                                key: RouteTyp::Fixed("B".to_string()),
                                item: Some(0),
                                childs: vec![]
                            },
                            Route {
                                key: RouteTyp::Fixed("C".to_string()),
                                item: Some(1),
                                childs: vec![]
                            }
                        ]
                    },]
                }
            }
        )
    }

    #[test]
    fn add_wrong_param() {
        let routes = Routes::new();
        let routes = routes.add("/:A/B", 0).unwrap();
        let error = routes.add("/:X/C", 1).unwrap_err();
        assert_eq!(
            error,
            RoutesError::ParamMismatch(":A".to_string(), ":X".to_string())
        )
    }

    #[test]
    fn add_no_param() {
        let routes = Routes::new();
        let routes = routes.add("/:A/B", 0).unwrap();
        let error = routes.add("/X/C", 1).unwrap_err();
        assert_eq!(
            error,
            RoutesError::ParamMismatch(":A".to_string(), "X".to_string())
        )
    }

    #[test]
    fn add_param() {
        let routes = Routes::new();
        let routes = routes.add("/A/B", 0).unwrap();
        let routes = routes.add("/:A/C", 1).unwrap();
        assert_eq!(
            routes,
            Routes {
                root: Route {
                    key: RouteTyp::Fixed("$root".to_string()),
                    item: None,
                    childs: vec![
                        Route {
                            key: RouteTyp::Fixed("A".to_string()),
                            item: None,
                            childs: vec![Route {
                                key: RouteTyp::Fixed("B".to_string()),
                                item: Some(0),
                                childs: vec![]
                            },]
                        },
                        Route {
                            key: RouteTyp::Param(":A".to_string()),
                            item: None,
                            childs: vec![Route {
                                key: RouteTyp::Fixed("C".to_string()),
                                item: Some(1),
                                childs: vec![]
                            }]
                        }
                    ]
                }
            }
        )
    }

    #[test]
    fn find_routes() {
        let routes = Routes::new();
        let routes = routes.add("/A/B", 0).unwrap();
        let routes = routes.add("/A", 1).unwrap();
        assert_eq!(routes.find("/A"), Some((1, HashMap::new())));
        assert_eq!(routes.find("/A/B"), Some((0, HashMap::new())));
        assert_eq!(routes.find("/C"), None);
        assert_eq!(routes.find("/A/C"), None);
    }

    #[test]
    fn find_params() {
        let routes = Routes::new();
        let routes = routes.add("/:A/B", 0).unwrap();
        let mut params = HashMap::new();
        params.insert("A".to_string(), "X".to_string());
        assert_eq!(routes.find("/X/B"), Some((0, params)))
    }
}
