// Utilities for signal processing, such as filtering and smoothing sensor readings.

pub struct MedianFilter3 <T> {
    readings: [T; 3],
    last_index: usize
}

impl<T> MedianFilter3 <T> where T: Default + Copy + Ord {
    pub fn new() -> Self {
        MedianFilter3 {
            readings: core::array::from_fn(|_| T::default()),
            last_index: 0,
        }
    }

    pub fn add_reading(&mut self, reading: T) -> T {
        self.readings[self.last_index] = reading;
        self.last_index = (self.last_index + 1) % self.readings.len();
        self.get_median()
    }

    pub fn get_median(&self) -> T {
        let first_reading = self.readings[0];
        let second_reading = self.readings[1];
        let third_reading = self.readings[2];

        let mut raw_readings = [first_reading, second_reading, third_reading];
        raw_readings.sort_unstable();
        raw_readings[1]
    }
}