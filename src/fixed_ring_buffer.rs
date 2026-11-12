use std::collections::VecDeque;

// A fixed-size ring buffer that stores the most recent readings. When the buffer is full,
// adding a new reading will remove the oldest reading to make space for the new one.
pub struct FixedRingBuffer <T> {
    readings: VecDeque<T>,
    vec_size: usize
}

impl<T> FixedRingBuffer <T> {
    pub fn new(buf_size: usize) -> Self {
        FixedRingBuffer {
            readings: VecDeque::with_capacity(buf_size),
            vec_size: buf_size
        }
    }

    pub fn add_reading(&mut self, reading: T) {
        if self.readings.len() == self.vec_size {
            self.readings.pop_front();
        }

        self.readings.push_back(reading);
    }

    pub fn get_all(&self, buffer: &mut [T; 60]) where T: Copy {
        for (i, reading) in self.readings.iter().enumerate() {
            buffer[i] = *reading;
        }
    }

    pub fn get_last(&self) -> Option<&T> {
        self.readings.back()
    }
}