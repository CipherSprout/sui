module 0x42::M {

    #[allow(lint(constant_naming))]
    const Another_BadName: u64 = 42; // Should trigger a warning

    #[allow(lint(collapsible_nested_if))]
    public fun nested_if_different_actions(x: bool, y: bool): bool {
        if (x) {
            if (y) {
                // Different action for y
                return true
            }
        };
        false
    }
}
