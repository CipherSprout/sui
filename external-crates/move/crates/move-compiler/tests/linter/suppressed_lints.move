module 0x42::M {

    #[allow(lint(constant_naming))]
    const Another_BadName: u64 = 42; // Should trigger a warning

    #[allow(lint(double_comparison))]
    fun func1(x: u64) {

        if (x < 5 || x > 10) {
        };

        if (x == 10 || x > 10) {
            
        };

        if (x == 10 || 10 > x) {};

    }
}
