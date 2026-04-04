pub mod error;
pub mod handlers;
pub mod middleware;
pub mod state;
pub mod util;

pub fn net_zero(amount: i64) -> i64 {
    -amount + amount
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_net_zero() {
        assert_eq!(net_zero(123), 0);
    }
}
