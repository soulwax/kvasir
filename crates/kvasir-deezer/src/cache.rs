use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use serde_json::Value;

#[derive(Clone, Debug)]
struct Entry {
    at: Instant,
    value: Value,
}

#[derive(Debug)]
pub struct TtlCache {
    max_size: usize,
    ttl: Duration,
    map: HashMap<String, Entry>,
    order: VecDeque<String>,
    hits: u64,
    misses: u64,
}

impl TtlCache {
    pub fn new(max_size: usize, ttl: Duration) -> Self {
        Self {
            max_size,
            ttl,
            map: HashMap::new(),
            order: VecDeque::new(),
            hits: 0,
            misses: 0,
        }
    }

    pub fn get(&mut self, key: &str) -> Option<Value> {
        let fresh = self
            .map
            .get(key)
            .is_some_and(|entry| entry.at.elapsed() < self.ttl);
        if fresh {
            self.hits += 1;
            self.map.get(key).map(|entry| entry.value.clone())
        } else {
            self.misses += 1;
            if self.map.contains_key(key) {
                self.map.remove(key);
            }
            None
        }
    }

    pub fn insert(&mut self, key: String, value: Value) {
        if self.map.contains_key(&key) {
            self.order.retain(|existing| existing != &key);
        }
        self.map.insert(
            key.clone(),
            Entry {
                at: Instant::now(),
                value,
            },
        );
        self.order.push_back(key);
        while self.map.len() > self.max_size {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            } else {
                break;
            }
        }
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }

    pub fn stats(&self) -> CacheBucketStats {
        CacheBucketStats {
            size: self.map.len(),
            max_size: self.max_size,
            hits: self.hits,
            misses: self.misses,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheBucketStats {
    pub size: usize,
    pub max_size: usize,
    pub hits: u64,
    pub misses: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheStats {
    pub shared: CacheBucketStats,
    pub public_api: CacheBucketStats,
}
