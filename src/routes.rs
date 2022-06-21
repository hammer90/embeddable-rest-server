#[derive(Debug, PartialEq, Eq)]
struct Route<T> {
    key: String,
    item: Option<T>,
    childs: Vec<Route<T>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RoutesError {
    RoutExists,
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
                key: curr.to_string(),
                item: None,
                childs: vec![Route::new(rest, item)],
            }
        } else {
            Self {
                key: path.to_string(),
                item: Some(item),
                childs: vec![],
            }
        }
    }

    fn find(&self, path: &str) -> Option<&Route<T>> {
        let path = uniform_path(path);
        if let Some((curr, rest)) = path.split_once('/') {
            if self.key == curr {
                for child in &self.childs {
                    let found = child.find(rest);
                    if let Some(_) = found {
                        return found;
                    }
                }
            }
        } else {
            if self.key == path {
                return Some(self);
            }
        }
        None
    }

    fn add(self, path: &str, item: T) -> Result<Route<T>, RoutesError> {
        if path == "" {
            if let Some(_) = self.item {
                return Err(RoutesError::RoutExists);
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
            if child.key == curr {
                new_childs.push(child.add(rest, item)?);
                added = true;
            } else {
                new_childs.push(child);
            }
        }
        if !added {
            new_childs.push(Route::new(path, item));
        }
        return Ok(Route {
            key: self.key,
            item: self.item,
            childs: new_childs,
        });
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
                key: "$root".to_string(),
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

    pub fn find(&self, path: &str) -> Option<T> {
        let path = uniform_path(path);
        let route = self.root.find(format!("$root/{}", path).as_str());
        if let Some(item) = route {
            return item.item;
        }
        None
    }
}

fn uniform_path(path: &str) -> &str {
    if path == "" || path == "/" {
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
                key: "A".to_string(),
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
                key: "A".to_string(),
                item: None,
                childs: vec![Route {
                    key: "B".to_string(),
                    item: Some(0),
                    childs: vec![]
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
            &Route {
                key: "A".to_string(),
                item: None,
                childs: vec![Route {
                    key: "B".to_string(),
                    item: Some(0),
                    childs: vec![]
                }]
            }
        );
        assert_eq!(
            route.find("/A/B").unwrap(),
            &Route {
                key: "B".to_string(),
                item: Some(0),
                childs: vec![]
            }
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
                    key: "$root".to_string(),
                    item: None,
                    childs: vec![Route {
                        key: "A".to_string(),
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
                    key: "$root".to_string(),
                    item: None,
                    childs: vec![Route {
                        key: "A".to_string(),
                        item: None,
                        childs: vec![Route {
                            key: "B".to_string(),
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
                    key: "$root".to_string(),
                    item: None,
                    childs: vec![
                        Route {
                            key: "A".to_string(),
                            item: Some(0),
                            childs: vec![]
                        },
                        Route {
                            key: "B".to_string(),
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
        assert_eq!(error, RoutesError::RoutExists)
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
                    key: "$root".to_string(),
                    item: None,
                    childs: vec![Route {
                        key: "A".to_string(),
                        item: None,
                        childs: vec![
                            Route {
                                key: "B".to_string(),
                                item: Some(0),
                                childs: vec![]
                            },
                            Route {
                                key: "C".to_string(),
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
        assert_eq!(error, RoutesError::RoutExists)
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
                    key: "$root".to_string(),
                    item: None,
                    childs: vec![Route {
                        key: "A".to_string(),
                        item: None,
                        childs: vec![Route {
                            key: "B".to_string(),
                            item: Some(0),
                            childs: vec![Route {
                                key: "C".to_string(),
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
                    key: "$root".to_string(),
                    item: None,
                    childs: vec![Route {
                        key: "A".to_string(),
                        item: Some(1),
                        childs: vec![Route {
                            key: "B".to_string(),
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
                    key: "$root".to_string(),
                    item: None,
                    childs: vec![Route {
                        key: "A".to_string(),
                        item: None,
                        childs: vec![Route {
                            key: "B".to_string(),
                            item: Some(1),
                            childs: vec![Route {
                                key: "C".to_string(),
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
                    key: "$root".to_string().to_string(),
                    item: Some(1),
                    childs: vec![Route {
                        key: "A".to_string(),
                        item: None,
                        childs: vec![Route {
                            key: "B".to_string(),
                            item: Some(0),
                            childs: vec![]
                        }]
                    },]
                }
            }
        )
    }

    #[test]
    fn find_routes() {
        let routes = Routes::new();
        let routes = routes.add("/A/B", 0).unwrap();
        let routes = routes.add("/A", 1).unwrap();
        assert_eq!(routes.find("/A"), Some(1));
        assert_eq!(routes.find("/A/B"), Some(0));
        assert_eq!(routes.find("/C"), None);
        assert_eq!(routes.find("/A/C"), None);
    }
}
