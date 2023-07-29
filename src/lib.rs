use davisjr::prelude::*;
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

pub trait HasLimit: TransientState {
    fn limiter(&self) -> Limiter;
}

pub type LimitMap = BTreeMap<String, (Duration, usize)>;

#[derive(Default, Clone, Debug)]
pub struct Limiter {
    map: Arc<Mutex<BTreeMap<String, BTreeMap<String, Vec<Instant>>>>>,
    limits: LimitMap,
}

impl Limiter {
    pub fn new(limits: LimitMap) -> Self {
        Self {
            limits,
            ..Default::default()
        }
    }

    pub async fn expire_limits(&self) -> Result<(), Error> {
        let mut lock = self.map.lock().await;

        for (_key, routes) in lock.iter_mut() {
            for (route, limitlist) in routes {
                if let Some(routelimit) = self.limits.get(route) {
                    let mut tmp = Vec::new();
                    for limit in &mut *limitlist {
                        if Instant::now().duration_since(*limit) < routelimit.0 {
                            tmp.push(limit.clone())
                        }
                    }

                    limitlist.clear();
                    limitlist.append(&mut tmp);
                }
            }
        }

        Ok(())
    }

    pub async fn process_request(&self, key: &str, route: &str) -> Result<(), Error> {
        let mut lock = self.map.lock().await;

        if let Some(keymap) = lock.get_mut(key) {
            if let Some(routemap) = keymap.get_mut(route) {
                if let Some(limit) = self.limits.get(route) {
                    if limit.1 > routemap.len() {
                        routemap.push(Instant::now());
                        return Ok(());
                    } else {
                        return Err(Error::new("API limit reached".to_string()));
                    }
                } else {
                    // no API limit
                    return Ok(());
                }
            } else {
                // no entries yet
                keymap.insert(route.to_string(), vec![Instant::now()]);
                return Ok(());
            }
        } else {
            // API key does not exist yet (create a new one)
            let mut item = BTreeMap::default();
            item.insert(route.to_string(), vec![Instant::now()]);
            lock.insert(key.to_string(), item);
            return Ok(());
        }
    }
}

pub async fn observe_limits(limiter: Limiter) -> Result<(), Error> {
    loop {
        limiter.expire_limits().await?;

        tokio::time::sleep(Duration::new(1, 0)).await;
    }
}

pub async fn with_limits(
    req: Request<Body>,
    resp: Option<Response<Body>>,
    _params: Params,
    app: App<impl HasLimit + 'static, NoState>,
    _state: NoState,
) -> HTTPResult<NoState> {
    let appstate = app.state().await.unwrap();
    let lock = appstate.lock().await;
    let limiter = lock.limiter();

    let route = req.uri().path();
    match req
        .headers()
        .get("Authorization")
        .map(|h| h.to_str().map(|s| s.trim_start_matches("Bearer ")))
    {
        Some(Ok(key)) => limiter.process_request(key, route).await?,
        Some(Err(e)) => return Err(e.into()),
        None => return Err(Error::new("no API key present".to_string())),
    }

    return Ok((req, resp, _state));
}

#[cfg(test)]
mod tests {
    use super::*;
    use davisjr::app::App;
    use davisjr::app::TestApp;
    use http::{HeaderMap, HeaderValue};

    #[derive(Clone, Default)]
    struct AppState {
        limiter: Limiter,
    }

    impl AppState {
        pub fn new(limiter: Limiter) -> Self {
            Self { limiter }
        }
    }

    impl TransientState for AppState {
        fn initial() -> Self {
            Default::default()
        }
    }

    impl HasLimit for AppState {
        fn limiter(&self) -> Limiter {
            self.limiter.clone()
        }
    }

    pub async fn mock_handler(
        req: Request<Body>,
        _resp: Option<Response<Body>>,
        _params: Params,
        _app: App<impl HasLimit + 'static, NoState>,
        _state: NoState,
    ) -> HTTPResult<NoState> {
        return Ok((
            req,
            Some(Response::builder().body(Body::empty()).unwrap()),
            _state,
        ));
    }

    #[tokio::test]
    async fn test_limits() {
        let mut map = LimitMap::default();

        map.insert("/test1".to_string(), (Duration::new(1, 0), 60));
        map.insert("/test2".to_string(), (Duration::new(1, 0), 10));
        map.insert("/test3".to_string(), (Duration::new(1, 0), 45));

        let limiter = Limiter::new(map.clone());

        let mut app: App<AppState, NoState> = App::with_state(AppState::new(limiter.clone()));

        app.get("/test1", compose_handler!(with_limits, mock_handler))
            .unwrap();
        app.get("/test2", compose_handler!(with_limits, mock_handler))
            .unwrap();
        app.get("/test3", compose_handler!(with_limits, mock_handler))
            .unwrap();

        tokio::spawn(async move { observe_limits(limiter).await.unwrap() });

        let testapp = TestApp::new(app);

        // no api key
        for (route, _) in &map {
            let res = testapp.get(&route).await;
            assert_eq!(res.status(), 500);
        }

        let mut headers = HeaderMap::new();
        let value = HeaderValue::from_str("Bearer foo").unwrap();
        headers.insert("Authorization", value);
        let testapp = testapp.with_headers(headers);

        for (route, val) in &map {
            // exhaust requests
            for _ in 0..val.1 {
                let res = testapp.get(&route).await;
                assert_eq!(res.status(), 200);
            }

            // should be error now
            let res = testapp.get(&route).await;
            assert_eq!(res.status(), 500);
        }

        // sleep and repeat, should be free to request again
        tokio::time::sleep(Duration::new(2, 0)).await;

        for (route, val) in &map {
            for _ in 0..val.1 {
                let res = testapp.get(&route).await;
                assert_eq!(res.status(), 200);
            }

            let res = testapp.get(&route).await;
            assert_eq!(res.status(), 500);
        }

        // no wait; change api key & retry, should work
        let mut headers = HeaderMap::new();
        let value = HeaderValue::from_str("Bearer bar").unwrap();
        headers.insert("Authorization", value);
        let testapp = testapp.with_headers(headers);

        for (route, val) in &map {
            for _ in 0..val.1 {
                let res = testapp.get(&route).await;
                assert_eq!(res.status(), 200);
            }

            let res = testapp.get(&route).await;
            assert_eq!(res.status(), 500);
        }
    }
}
