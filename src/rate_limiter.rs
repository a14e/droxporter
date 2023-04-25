use chrono::{DateTime, Duration, Utc};

// multi level implementation of https://en.wikipedia.org/wiki/Leaky_bucket
#[derive(Copy, Clone, Debug)]
pub struct MultiLimits<const Limits: usize> {
    limits: [RateLimiter; Limits],
}

type Limit = usize;

impl<const Limits: usize> MultiLimits<Limits> {
    pub fn new<TimeInterval: ToMillis, Time: ToMillis>(limit_settings: [(Limit, TimeInterval); Limits],
                                                       current_time: Time) -> Self {
        let mut limits = [RateLimiter::default(); Limits];
        for (idx, (limit, interval)) in limit_settings.into_iter().enumerate() {
            limits[idx] = RateLimiter::new(
                limit,
                interval.to_millis(),
                current_time.to_millis(),
            );
        }
        Self { limits }
    }

    pub fn can_acquire<Time: ToMillis>(&self,
                                       current_time: Time) -> bool {
        let millis = current_time.to_millis();
        self.limits
            .iter()
            .all(|x| x.can_acquire(millis))
    }


    pub fn acquire<Time: ToMillis>(&mut self,
                                   current_time: Time) -> bool {
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
    limit_time_interval: usize,
    multiplication_ratio: f32
}

impl RateLimiter {
    pub fn new(limit: usize,
               limit_interval: usize,
               current_time: usize) -> Self {
        let multiplication_ratio = limit as f32 / limit_interval as f32;
        Self {
            remaining: limit,
            limit,
            limit_time_interval: limit_interval,
            last_attempt_time: current_time,
            multiplication_ratio
        }
    }


    pub fn can_acquire(&self,
                       current_time: usize) -> bool {
        self.estimate_remaining(current_time) > 0
    }

    fn estimate_remaining(&self,
                          current_time: usize) -> usize {
        let time_diff = current_time.saturating_sub(self.last_attempt_time);

        let next_count = self.remaining +
            (time_diff as f32 * self.multiplication_ratio) as usize;

        let next_count = next_count.min(self.limit);

        if next_count > 0 {
            next_count - 1
        } else {
            0
        }
    }

    pub fn acquire(&mut self,
                   current_time: usize) -> bool {
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

impl ToMillis for DateTime<Utc> {
    fn to_millis(&self) -> usize {
        DateTime::timestamp_millis(&self) as usize
    }
}

impl ToMillis for Duration {
    fn to_millis(&self) -> usize {
        self.num_milliseconds() as usize
    }
}