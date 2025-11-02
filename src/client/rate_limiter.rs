use chrono::{DateTime, Duration, Utc};

// multi level implementation of https://en.wikipedia.org/wiki/Leaky_bucket
#[derive(Copy, Clone, Debug)]
pub struct MultiLimits<const LIMITS: usize> {
    limits: [RateLimiter; LIMITS],
}

type Limit = usize;

impl<const LIMITS: usize> MultiLimits<LIMITS> {
    pub fn new<TimeInterval: ToMillis, Time: ToMillis>(
        limit_settings: [(Limit, TimeInterval); LIMITS],
        current_time: Time,
    ) -> Self {
        let mut limits = [RateLimiter::default(); LIMITS];
        for (idx, (limit, interval)) in limit_settings.into_iter().enumerate() {
            limits[idx] = RateLimiter::new(limit, interval.to_millis(), current_time.to_millis());
        }
        Self { limits }
    }

    pub fn can_acquire<Time: ToMillis>(&self, current_time: Time) -> bool {
        let millis = current_time.to_millis();
        self.limits.iter().all(|x| x.can_acquire(millis))
    }

    pub fn estimate_remaining<Time: ToMillis>(&self, idx: usize, current_time: Time) -> usize {
        if idx >= LIMITS {
            return 0;
        }
        self.limits[idx].estimate_remaining(current_time.to_millis())
    }

    pub fn acquire<Time: ToMillis>(&mut self, current_time: Time) -> bool {
        let millis = current_time.to_millis();
        if !self.can_acquire(millis) {
            return false;
        }
        for limit in &mut self.limits {
            let _ = limit.acquire(millis);
        }
        true
    }
}

#[derive(Default, Copy, Clone, Debug)]
pub struct RateLimiter {
    remaining: usize,
    last_attempt_time: usize,
    limit: usize,
    multiplication_ratio: f32,
}

impl RateLimiter {
    pub fn new(limit: usize, limit_interval: usize, current_time: usize) -> Self {
        let multiplication_ratio = limit as f32 / limit_interval as f32;
        Self {
            remaining: 0,
            limit,
            last_attempt_time: current_time,
            multiplication_ratio,
        }
    }

    pub fn can_acquire(&self, current_time: usize) -> bool {
        self.estimate_remaining(current_time) > 0
    }

    pub fn estimate_remaining(&self, current_time: usize) -> usize {
        let time_diff = current_time.saturating_sub(self.last_attempt_time);

        let next_count = self.remaining + (time_diff as f32 * self.multiplication_ratio) as usize;

        let next_count = next_count.min(self.limit);

        if next_count > 0 { next_count - 1 } else { 0 }
    }

    pub fn acquire(&mut self, current_time: usize) -> bool {
        let next_count = self.estimate_remaining(current_time);
        if next_count == 0 {
            return false;
        }
        self.remaining = next_count;
        self.last_attempt_time = current_time;
        true
    }
}

pub trait ToMillis {
    fn to_millis(&self) -> usize;
}

impl ToMillis for usize {
    fn to_millis(&self) -> usize {
        *self
    }
}

impl ToMillis for u64 {
    fn to_millis(&self) -> usize {
        *self as usize
    }
}

impl ToMillis for i32 {
    fn to_millis(&self) -> usize {
        (*self).max(0) as usize
    }
}

impl ToMillis for DateTime<Utc> {
    fn to_millis(&self) -> usize {
        DateTime::timestamp_millis(self) as usize
    }
}

impl ToMillis for Duration {
    fn to_millis(&self) -> usize {
        self.num_milliseconds() as usize
    }
}

#[cfg(test)]
#[allow(clippy::module_inception)]
mod rate_limiter {
    use crate::client::rate_limiter::RateLimiter;

    #[test]
    fn pass_values_under_limit() {
        let limiter = RateLimiter::new(10, 10, 0);
        assert!(limiter.can_acquire(10))
    }

    #[test]
    fn dont_pass_on_exceeded_limit() {
        let mut limiter = RateLimiter::new(10, 10, 0);
        for _ in 0..100 {
            let _ = limiter.acquire(10);
        }
        assert!(!limiter.can_acquire(10))
    }

    #[test]
    fn pass_exceeded_after_timeout() {
        let mut limiter = RateLimiter::new(10, 10, 0);
        for _ in 0..100 {
            let _ = limiter.acquire(10);
        }
        assert!(limiter.can_acquire(20))
    }
}

#[cfg(test)]
mod multi_limits {
    use crate::client::rate_limiter::MultiLimits;

    #[test]
    fn pass_values_under_limit() {
        let limiter = MultiLimits::new([(10, 10), (100, 100)], 0);
        assert!(limiter.can_acquire(2))
    }

    #[test]
    fn dont_pass_on_exceeded_small_limit() {
        let mut limiter = MultiLimits::new([(10, 10), (100, 100)], 0);
        for _ in 0..100 {
            let _ = limiter.acquire(10);
        }
        assert!(!limiter.can_acquire(10))
    }

    #[test]
    fn pass_exceeded_after_timeout_small_limit() {
        let mut limiter = MultiLimits::new([(10, 10), (100, 100)], 0);
        for _ in 0..100 {
            let _ = limiter.acquire(10);
        }
        assert!(limiter.can_acquire(20))
    }

    #[test]
    fn dont_pass_on_exceeded_big_limit() {
        let mut limiter = MultiLimits::new([(10, 10), (100, 1000)], 0);
        for i in 0..=1000 {
            let _ = limiter.acquire(i);
        }
        assert!(!limiter.can_acquire(1000))
    }

    #[test]
    fn pass_exceeded_after_timeout_big_limit() {
        let mut limiter = MultiLimits::new([(10, 10), (100, 1000)], 0);
        for i in 0..=100 {
            let _ = limiter.acquire(i * 10);
        }
        assert!(limiter.can_acquire(1010))
    }
}
