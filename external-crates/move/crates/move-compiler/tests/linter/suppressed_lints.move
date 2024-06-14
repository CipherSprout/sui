module 0x42::M {

    #[allow(lint(constant_naming))]
    const Another_BadName: u64 = 42; // Should trigger a warning

    #[allow(lint(multiplication_overflow))]
    public fun potential_overflow_multiplication() {
        let _a: u64 = 1_000_000_000_000_000;
        let _b: u64 = 2_000_000_000_000_000;
        let _ = 1_000_000_000_000_000 * 2_000_000_000_000_000; // Should trigger a warning
    }
}
