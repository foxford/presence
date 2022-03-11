use std::env::var;
use svc_authz::cache::RedisCache;
use svc_authz::IntentObject;

pub type AuthzCache = Box<dyn svc_authz::cache::AuthzCache>;

#[derive(Clone)]
pub struct AuthzObject {
    object: Vec<String>,
}

impl AuthzObject {
    pub fn new(obj: &[&str]) -> Self {
        Self {
            object: obj.iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl IntentObject for AuthzObject {
    fn to_ban_key(&self) -> Option<Vec<String>> {
        None
    }

    fn to_vec(&self) -> Vec<String> {
        self.object.clone()
    }

    fn box_clone(&self) -> Box<dyn IntentObject> {
        Box::new(self.clone())
    }
}

impl From<AuthzObject> for Box<dyn IntentObject> {
    fn from(o: AuthzObject) -> Self {
        Box::new(o)
    }
}

pub fn new_cache() -> Option<AuthzCache> {
    let cache_enabled = var("CACHE_ENABLED").ok()?;

    if cache_enabled != "1" {
        return None;
    }

    let cache = new_redis_cache();
    Some(cache)
}

fn new_redis_cache() -> AuthzCache {
    let url = var("CACHE_URL").expect("CACHE_URL must be specified");

    let size = var("CACHE_POOL_SIZE")
        .map(|val| {
            val.parse::<u32>()
                .expect("Error converting CACHE_POOL_SIZE variable into u32")
        })
        .unwrap_or(5);

    let idle_size = var("CACHE_POOL_IDLE_SIZE")
        .map(|val| {
            val.parse::<u32>()
                .expect("Error converting CACHE_POOL_IDLE_SIZE variable into u32")
        })
        .ok();

    let timeout = var("CACHE_POOL_TIMEOUT")
        .map(|val| {
            val.parse::<u64>()
                .expect("Error converting CACHE_POOL_TIMEOUT variable into u64")
        })
        .unwrap_or(5);

    let expiration_time = var("CACHE_EXPIRATION_TIME")
        .map(|val| {
            val.parse::<usize>()
                .expect("Error converting CACHE_EXPIRATION_TIME variable into u64")
        })
        .unwrap_or(300);

    let pool = svc_authz::cache::create_pool(&url, size, idle_size, timeout);
    Box::new(RedisCache::new(pool, expiration_time))
}
