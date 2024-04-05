CREATE TABLE tx_senders (
    cp_sequence_number          BIGINT       NOT NULL,
    tx_sequence_number          BIGINT       NOT NULL,
    -- SuiAddress in bytes.
    sender                      BYTEA        NOT NULL,
    PRIMARY KEY(sender, tx_sequence_number, cp_sequence_number)
) PARTITION BY RANGE (cp_sequence_number);
CREATE TABLE tx_senders_partition_0 PARTITION OF tx_senders FOR VALUES FROM (0) TO (MAXVALUE);
CREATE INDEX tx_senders_tx_sequence_number_index ON tx_senders (tx_sequence_number, cp_sequence_number);

CREATE TABLE tx_recipients (
    cp_sequence_number          BIGINT       NOT NULL,
    tx_sequence_number          BIGINT       NOT NULL,
    -- SuiAddress in bytes.
    recipient                   BYTEA        NOT NULL,
    PRIMARY KEY(recipient, tx_sequence_number, cp_sequence_number)
) PARTITION BY RANGE (cp_sequence_number);
CREATE TABLE tx_recipients_partition_0 PARTITION OF tx_recipients FOR VALUES FROM (0) TO (MAXVALUE);
CREATE INDEX tx_recipients_tx_sequence_number_index ON tx_recipients (tx_sequence_number, cp_sequence_number);

CREATE TABLE tx_input_objects (
    cp_sequence_number          BIGINT       NOT NULL,
    tx_sequence_number          BIGINT       NOT NULL,
    -- Object ID in bytes. 
    object_id                   BYTEA        NOT NULL,
    PRIMARY KEY(object_id, tx_sequence_number, cp_sequence_number)
) PARTITION BY RANGE (cp_sequence_number);
CREATE TABLE tx_input_objects_partition_0 PARTITION OF tx_input_objects FOR VALUES FROM (0) TO (MAXVALUE);

CREATE TABLE tx_changed_objects (
    cp_sequence_number          BIGINT       NOT NULL,
    tx_sequence_number          BIGINT       NOT NULL,
    -- Object Id in bytes.
    object_id                   BYTEA        NOT NULL,
    PRIMARY KEY(object_id, tx_sequence_number, cp_sequence_number)
) PARTITION BY RANGE (cp_sequence_number);
CREATE TABLE tx_changed_objects_partition_0 PARTITION OF tx_changed_objects FOR VALUES FROM (0) TO (MAXVALUE);

CREATE TABLE tx_calls (
    cp_sequence_number          BIGINT       NOT NULL,
    tx_sequence_number          BIGINT       NOT NULL,
    package                     BYTEA        NOT NULL,
    module                      TEXT         NOT NULL,
    func                        TEXT         NOT NULL,
    -- 1. Using Primary Key as a unique index.
    -- 2. Diesel does not like tables with no primary key.
    PRIMARY KEY(package, tx_sequence_number, cp_sequence_number)
) PARTITION BY RANGE (cp_sequence_number);
CREATE TABLE tx_calls_partition_0 PARTITION OF tx_calls FOR VALUES FROM (0) TO (MAXVALUE);
CREATE INDEX tx_calls_module ON tx_calls (package, module, tx_sequence_number, cp_sequence_number);
CREATE INDEX tx_calls_func ON tx_calls (package, module, func, tx_sequence_number, cp_sequence_number);
CREATE INDEX tx_calls_tx_sequence_number ON tx_calls (tx_sequence_number, cp_sequence_number);
