module 0x42::M {

    #[allow(lint(constant_naming))]
    const Another_BadName: u64 = 42; // Should trigger a warning

    #[allow(lint(redundant_conditional))]
    fun func1() {
        let x = true;
        if (x) {
            false
        } else {
            true
        };

        if (x) {
            true
        } else {
            false
        };

        if (foo()) true else false;
        if (foo()) (true) else (false);
    }

    fun foo(): bool {
        true
    }
}
