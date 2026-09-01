#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Order {
    Sequential,
    Shuffle,
}

pub fn parse_order(name: &str) -> Order {
    if name == "sequential" { Order::Sequential } else { Order::Shuffle }
}

fn xorshift64(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

pub fn step(order: Order, cursor: usize, len: usize, forward: bool, rng: &mut u64) -> usize {
    if len <= 1 {
        return 0;
    }
    match order {
        Order::Sequential => {
            if forward {
                (cursor + 1) % len
            } else {
                (cursor + len - 1) % len
            }
        }
        Order::Shuffle => {
            let mut next = (xorshift64(rng) as usize) % len;
            if next == cursor {
                next = (next + 1) % len;
            }
            next
        }
    }
}

#[cfg(test)]
#[path = "order_tests.rs"]
mod tests;
