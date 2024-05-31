module 0x42::M {

    #[allow(lint(constant_naming))]
    const Another_BadName: u64 = 42; // Should trigger a warning

    #[allow(lint(redundant_ref_deref))] 
    public fun test_borrow_deref_ref() {
        let resource = MyResource { value: 10 };
        let _ref2 = &*(&resource);
    }
}
