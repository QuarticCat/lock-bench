#![allow(clippy::declare_interior_mutable_const)]

mod naive;

use std::{iter, sync::Barrier, time};

use crossbeam_utils::{thread::scope, CachePadded};

fn main() {
    let mut args = std::env::args()
        .skip(1)
        .map(|it| it.parse::<u32>().unwrap());

    let options = Options {
        n_threads: args.next().unwrap(),
        n_locks: args.next().unwrap(),
        n_ops: args.next().unwrap(),
        n_rounds: args.next().unwrap(),
    };
    println!("{:#?}\n", options);

    bench::<mutexes::Std>("std::sync::Mutex", &options);
    bench::<mutexes::ParkingLot>("parking_lot::Mutex", &options);
    bench::<mutexes::Spin>("spin::Mutex", &options);
    bench::<mutexes::NaiveSpin>("Spinlock (naive)", &options);
}

fn bench<M: Mutex>(label: &str, options: &Options) {
    let mut times = (0..options.n_rounds)
        .map(|_| run_bench::<M>(options))
        .collect::<Vec<_>>();
    times.sort();

    let avg = times.iter().sum::<time::Duration>() / options.n_rounds;
    let min = times[0];
    let max = *times.last().unwrap();

    let avg = format!("{:?}", avg);
    let min = format!("{:?}", min);
    let max = format!("{:?}", max);

    println!(
        "{:<20} avg {:<12} min {:<12} max {:<12}",
        label, avg, min, max
    )
}

#[derive(Debug)]
struct Options {
    n_threads: u32,
    n_locks: u32,
    n_ops: u32,
    n_rounds: u32,
}

fn random_numbers(seed: u32) -> impl Iterator<Item = u32> {
    let mut random = seed;
    iter::repeat_with(move || {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        random
    })
}

trait Mutex: Sync + Send + Default {
    fn with_lock(&self, f: impl FnOnce(&mut u32));
}

fn run_bench<M: Mutex>(options: &Options) -> time::Duration {
    let locks = &(0..options.n_locks)
        .map(|_| CachePadded::new(M::default()))
        .collect::<Vec<_>>();

    let start_barrier = &Barrier::new(options.n_threads as usize + 1);
    let end_barrier = &Barrier::new(options.n_threads as usize + 1);

    let elapsed = scope(|scope| {
        let thread_seeds = random_numbers(0x6F4A955E).scan(0x9BA2BF27, |state, n| {
            *state ^= n;
            Some(*state)
        });
        for thread_seed in thread_seeds.take(options.n_threads as usize) {
            scope.spawn(move |_| {
                start_barrier.wait();
                let indexes = random_numbers(thread_seed)
                    .map(|it| it % options.n_locks)
                    .map(|it| it as usize)
                    .take(options.n_ops as usize);
                for idx in indexes {
                    locks[idx].with_lock(|cnt| *cnt += 1);
                }
                end_barrier.wait();
            });
        }

        std::thread::sleep(time::Duration::from_millis(100));
        start_barrier.wait();
        let start = time::Instant::now();
        end_barrier.wait();
        let elapsed = start.elapsed();

        let mut total = 0;
        for lock in locks.iter() {
            lock.with_lock(|cnt| total += *cnt);
        }
        assert_eq!(total, options.n_threads * options.n_ops);

        elapsed
    })
    .unwrap();
    elapsed
}

mod mutexes {
    use super::Mutex;

    pub(crate) type Std = std::sync::Mutex<u32>;
    pub(crate) type ParkingLot = lock_api::Mutex<parking_lot::RawMutex, u32>;
    pub(crate) type Spin = lock_api::Mutex<spin::mutex::Mutex<()>, u32>;
    pub(crate) type NaiveSpin = lock_api::Mutex<crate::naive::RawSpinlock, u32>;

    impl Mutex for Std {
        fn with_lock(&self, f: impl FnOnce(&mut u32)) {
            let mut guard = self.lock().unwrap();
            f(&mut guard)
        }
    }

    impl<T: lock_api::RawMutex + Sync + Send> Mutex for lock_api::Mutex<T, u32> {
        fn with_lock(&self, f: impl FnOnce(&mut u32)) {
            let mut guard = self.lock();
            f(&mut guard)
        }
    }
}
