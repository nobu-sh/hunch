/// Regroups a stream of samples into chunks of one fixed size.
pub struct Chunker {
    size: usize,
    buffer: Vec<f32>,
}

impl Chunker {
    pub fn new(size: usize) -> Self {
        let size = size.max(1);
        Self {
            size,
            buffer: Vec::with_capacity(size),
        }
    }

    /// Calls `on_chunk` for each full chunk and keeps the remainder for the next push.
    pub fn push(&mut self, mut samples: &[f32], mut on_chunk: impl FnMut(&[f32])) {
        while !samples.is_empty() {
            let take = (self.size - self.buffer.len()).min(samples.len());
            self.buffer.extend_from_slice(&samples[..take]);
            samples = &samples[take..];
            if self.buffer.len() == self.size {
                on_chunk(&self.buffer);
                self.buffer.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_chunks(chunker: &mut Chunker, samples: &[f32]) -> Vec<Vec<f32>> {
        let mut chunks = Vec::new();
        chunker.push(samples, |chunk| chunks.push(chunk.to_vec()));
        chunks
    }

    #[test]
    fn emits_full_chunks_and_holds_the_rest() {
        let mut chunker = Chunker::new(3);

        let chunks = collect_chunks(&mut chunker, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);

        assert_eq!(chunks, [vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]]);
    }

    #[test]
    fn joins_samples_across_pushes() {
        let mut chunker = Chunker::new(3);

        assert!(collect_chunks(&mut chunker, &[1.0, 2.0]).is_empty());
        let chunks = collect_chunks(&mut chunker, &[3.0, 4.0]);

        assert_eq!(chunks, [vec![1.0, 2.0, 3.0]]);
    }
}
