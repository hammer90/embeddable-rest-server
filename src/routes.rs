#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route<'a, T> {
    key: &'a str,
    item: Option<&'a T>,
    childs: Vec<Route<'a, T>>,
}

impl<'a, T> Route<'a, T> {
    pub fn root() -> Self {
        Self {
            key: "",
            item: None,
            childs: vec![],
        }
    }

    pub fn new(path: &'a str, item: &'a T) -> Self {
        let path = uniform_path(path);
        if let Some((curr, rest)) = path.split_once('/') {
            Self {
                key: curr,
                item: None,
                childs: vec![Route::new(rest, item)],
            }
        } else {
            Self {
                key: path,
                item: Some(item),
                childs: vec![],
            }
        }
    }

    pub fn find(&self, path: &str) -> Option<&Route<T>> {
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

    pub fn add(self, path: &'a str, item: &'a T) -> Result<Route<'a, T>, ()> {
        let path = uniform_path(path);
        if let Some((curr, rest)) = path.split_once('/') {
            let mut new_childs = vec![];
            let mut added = false;
            for child in self.childs {
                if child.key == curr {
                    println!("{} == {}", child.key, curr);
                    new_childs.push(child.add(rest, item)?);
                    added = true;
                } else {
                    println!("{} != {}", child.key, curr);
                    new_childs.push(child);
                }
            }
            if !added {
                new_childs.push(Route::new(rest, item));
            }
            return Ok(Route {
                key: self.key,
                item: self.item,
                childs: new_childs,
            });
        } else {
            debug_assert_eq!(self.key, path);
            if let Some(_) = self.item {
                return Err(());
            }
            return Ok(Route {
                key: self.key,
                item: Some(item),
                childs: self.childs,
            });
        }
    }
}

fn uniform_path<'a>(path: &'a str) -> &'a str {
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
    fn new_root() {
        let route = Route::new("/A", &0);
        assert_eq!(
            route,
            Route {
                key: "A",
                item: Some(&0),
                childs: vec![]
            }
        );
    }

    #[test]
    fn new_child() {
        let route = Route::new("/A/B", &0);
        assert_eq!(
            route,
            Route {
                key: "A",
                item: None,
                childs: vec![Route {
                    key: "B",
                    item: Some(&0),
                    childs: vec![]
                }]
            }
        );
    }

    #[test]
    fn find() {
        let route = Route::new("/A/B", &0);
        assert_eq!(route.find("/C"), None);
        assert_eq!(
            route.find("/A").unwrap().to_owned(),
            Route {
                key: "A",
                item: None,
                childs: vec![Route {
                    key: "B",
                    item: Some(&0),
                    childs: vec![]
                }]
            }
        );
        assert_eq!(
            route.find("/A/B").unwrap().to_owned(),
            Route {
                key: "B",
                item: Some(&0),
                childs: vec![]
            }
        );
        assert_eq!(route.find("/A/C"), None);
    }

    #[test]
    fn add_first() {
        let root: Route<u32> = Route::root();
        let route = root.add("root/A", &0).unwrap();
        assert_eq!(
            route,
            Route {
                key: "",
                item: None,
                childs: vec![Route {
                    key: "A",
                    item: Some(&0),
                    childs: vec![]
                }]
            }
        )
    }

    #[test]
    fn add_first_deep() {
        let root: Route<u32> = Route::root();
        let route = root.add("root/A/B", &0).unwrap();
        assert_eq!(
            route,
            Route {
                key: "",
                item: None,
                childs: vec![Route {
                    key: "A",
                    item: None,
                    childs: vec![Route {
                        key: "B",
                        item: Some(&0),
                        childs: vec![]
                    }]
                }]
            }
        )
    }

    #[test]
    fn add_second() {
        let root: Route<u32> = Route::root();
        let route1 = root.add("root/A", &0).unwrap();
        let route2 = route1.add("root/B", &1).unwrap();
        assert_eq!(
            route2,
            Route {
                key: "",
                item: None,
                childs: vec![
                    Route {
                        key: "A",
                        item: Some(&0),
                        childs: vec![]
                    },
                    Route {
                        key: "B",
                        item: Some(&1),
                        childs: vec![]
                    }
                ]
            }
        )
    }

    #[test]
    fn add_second_deep() {
        let root: Route<u32> = Route::root();
        let route1 = root.add("root/A/B", &0).unwrap();
        let route2 = route1.add("root/A/C", &1).unwrap();
        assert_eq!(
            route2,
            Route {
                key: "",
                item: None,
                childs: vec![Route {
                    key: "A",
                    item: None,
                    childs: vec![
                        Route {
                            key: "B",
                            item: Some(&0),
                            childs: vec![]
                        },
                        Route {
                            key: "C",
                            item: Some(&1),
                            childs: vec![]
                        }
                    ]
                }]
            }
        )
    }

    #[test]
    fn add_sibling() {
        let route = Route::new("/A/B", &0);
        let added = route.add("/A/C", &1).unwrap();
        assert_eq!(
            added,
            Route {
                key: "A",
                item: None,
                childs: vec![
                    Route {
                        key: "B",
                        item: Some(&0),
                        childs: vec![]
                    },
                    Route {
                        key: "C",
                        item: Some(&1),
                        childs: vec![]
                    },
                ]
            }
        );
    }

    #[test]
    fn add_child() {
        let route = Route::new("/A/B", &0);
        let added = route.add("/A/B/C", &1).unwrap();
        assert_eq!(
            added,
            Route {
                key: "A",
                item: None,
                childs: vec![Route {
                    key: "B",
                    item: Some(&0),
                    childs: vec![Route {
                        key: "C",
                        item: Some(&1),
                        childs: vec![]
                    }]
                },]
            }
        );
    }
}
