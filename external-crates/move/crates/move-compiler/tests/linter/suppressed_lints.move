module 0x42::M {

    use sui::object::UID;
    use sui::transfer;

    struct AdminCap has key {
       id: UID
    }

    #[allow(lint(freezing_capability))]
    public fun freeze_cap(w: AdminCap) {
        transfer::public_freeze_object(w);
    }

    #[allow(lint(constant_naming))]
    const Another_BadName: u64 = 42; // Should trigger a warning
}

module sui::object {
    struct UID has store {
        id: address,
    }
}

module sui::transfer {
    public fun public_freeze_object<T: key>(_: T) {
        abort 0
    }
}