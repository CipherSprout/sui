module 0x42::M {

    #[allow(lint(constant_naming))]
    const Another_BadName: u64 = 42; // Should trigger a warning

    #[allow(lint(shift_overflow))]
    fun func1(x: u64) {
        let _b = x << 64; // Should raise an issue
        let _b = x << 65; // Should raise an issue
        let _b = x >> 66; // Should raise an issue
    }
}
