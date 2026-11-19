//! Optimistic parallel batches (Block-STM style, simplified).
//!
//! Every transaction in a batch first runs in parallel against the state at
//! the start of the batch, recording what it read and wrote. Then the batch
//! commits in order: a transaction whose reads touch anything an earlier
//! transaction in the batch wrote runs again on the current state. The final
//! state always equals running the transactions one by one.

use seal_jobs::JobPool;

/// A transaction's window onto the accounts: reads are recorded, writes buffered.
pub struct Access<'a> {
    state: &'a [i64],
    reads: Vec<usize>,
    writes: Vec<(usize, i64)>,
}

impl Access<'_> {
    pub fn read(&mut self, account: usize) -> i64 {
        if let Some(&(_, v)) = self.writes.iter().rev().find(|(a, _)| *a == account) {
            return v;
        }
        self.reads.push(account);
        self.state[account]
    }

    pub fn write(&mut self, account: usize, value: i64) {
        self.writes.push((account, value));
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BatchStats {
    pub transactions: usize,
    pub reexecuted: usize,
}

impl BatchStats {
    pub fn reexecuted_share(&self) -> f64 {
        if self.transactions == 0 {
            0.0
        } else {
            self.reexecuted as f64 / self.transactions as f64
        }
    }
}

/// What one run read, and what it wants to write.
type Trace = (Vec<usize>, Vec<(usize, i64)>);

fn run_one<F: Fn(usize, &mut Access)>(state: &[i64], index: usize, txn: &F) -> Trace {
    let mut access = Access { state, reads: Vec::new(), writes: Vec::new() };
    txn(index, &mut access);
    (access.reads, access.writes)
}

/// Run transactions `first..first + count` against `state`, `batch` at a time.
pub fn execute_batch<F>(
    pool: &JobPool,
    state: &mut [i64],
    first: usize,
    count: usize,
    batch: usize,
    txn: F,
) -> BatchStats
where
    F: Fn(usize, &mut Access) + Sync,
{
    let batch = batch.max(1);
    let mut stats = BatchStats { transactions: count, reexecuted: 0 };
    let mut written = vec![false; state.len()];
    let mut start = first;
    while start < first + count {
        let len = batch.min(first + count - start);
        let snapshot: &[i64] = state;
        let chunk = len.div_ceil(pool.workers().max(1) * 4).max(1);
        let results: Vec<Vec<Trace>> = pool.map_chunks(len, chunk, |_, range| {
            range.map(|i| run_one(snapshot, start + i, &txn)).collect()
        });
        let mut touched = Vec::new();
        for (offset, (reads, writes)) in results.into_iter().flatten().enumerate() {
            let conflict = reads.iter().any(|&a| written[a]);
            let writes = if conflict {
                stats.reexecuted += 1;
                run_one(state, start + offset, &txn).1
            } else {
                writes
            };
            for (account, value) in writes {
                state[account] = value;
                if !written[account] {
                    written[account] = true;
                    touched.push(account);
                }
            }
        }
        for account in touched {
            written[account] = false;
        }
        start += len;
    }
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cost::predicted_reexecution;

    fn transfer(accounts: usize) -> impl Fn(usize, &mut Access) + Sync {
        move |i, access: &mut Access| {
            let h = (i as u64).wrapping_mul(0x9E3779B97F4A7C15);
            let from = (h % accounts as u64) as usize;
            let to = ((h >> 32) % accounts as u64) as usize;
            let amount = (h >> 50) as i64 % 7;
            let a = access.read(from);
            if a >= amount {
                access.write(from, a - amount);
                let b = access.read(to);
                access.write(to, b + amount);
            }
        }
    }

    fn serial(accounts: usize, count: usize) -> Vec<i64> {
        let mut state = vec![100; accounts];
        let txn = transfer(accounts);
        for i in 0..count {
            let (_, writes) = run_one(&state, i, &txn);
            for (a, v) in writes {
                state[a] = v;
            }
        }
        state
    }

    #[test]
    fn always_equals_serial_execution() {
        let pool = JobPool::new(4);
        for accounts in [10usize, 1000, 100_000] {
            let mut state = vec![100; accounts];
            let stats = execute_batch(&pool, &mut state, 0, 20_000, 1024, transfer(accounts));
            assert_eq!(state, serial(accounts, 20_000), "accounts {accounts}");
            assert_eq!(stats.transactions, 20_000);
        }
    }

    #[test]
    fn measured_conflicts_follow_the_model() {
        let pool = JobPool::new(4);
        let accounts = 100_000;
        let mut state = vec![100; accounts];
        let stats = execute_batch(&pool, &mut state, 0, 8192 * 8, 8192, transfer(accounts));
        let predicted = predicted_reexecution(accounts as f64, 2.0, 8192.0);
        assert!(
            (stats.reexecuted_share() - predicted).abs() < 0.08,
            "{} vs {predicted}",
            stats.reexecuted_share()
        );
    }
}
