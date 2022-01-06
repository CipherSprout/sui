// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use crate::downloader::*;
use anyhow::{bail, ensure};
use fastx_types::{
    base_types::*, committee::Committee, error::FastPayError, fp_ensure, messages::*,
};
use futures::{future, StreamExt, TryFutureExt};
use rand::seq::SliceRandom;
use std::collections::{btree_map, BTreeMap, BTreeSet, HashMap};
use std::time::Duration;
use tokio::time::timeout;

#[cfg(test)]
#[path = "unit_tests/client_tests.rs"]
mod client_tests;

// TODO: Make timeout duration configurable.
const AUTHORITY_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

pub type AsyncResult<'a, T, E> = future::BoxFuture<'a, Result<T, E>>;

pub trait AuthorityClient {
    /// Initiate a new order to a FastPay or Primary account.
    fn handle_order(&mut self, order: Order) -> AsyncResult<'_, ObjectInfoResponse, FastPayError>;

    /// Confirm an order to a FastPay or Primary account.
    fn handle_confirmation_order(
        &mut self,
        order: ConfirmationOrder,
    ) -> AsyncResult<'_, ObjectInfoResponse, FastPayError>;

    /// Handle Account information requests for this account.
    fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> AsyncResult<'_, AccountInfoResponse, FastPayError>;

    /// Handle Object information requests for this account.
    fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> AsyncResult<'_, ObjectInfoResponse, FastPayError>;
}

pub struct ClientState<AuthorityClient> {
    /// Our FastPay address.
    address: FastPayAddress,
    /// Our signature key.
    secret: KeyPair,
    /// Our FastPay committee.
    committee: Committee,
    /// How to talk to this committee.
    authority_clients: HashMap<AuthorityName, AuthorityClient>,
    /// Pending transfer.
    pending_transfer: Option<Order>,

    // The remaining fields are used to minimize networking, and may not always be persisted locally.
    /// Transfer certificates that we have created ("sent").
    /// Normally, `sent_certificates` should contain one certificate for each index in `0..next_sequence_number`.
    sent_certificates: Vec<CertifiedOrder>,
    /// Known received certificates, indexed by sender and sequence number.
    /// TODO: API to search and download yet unknown `received_certificates`.
    received_certificates: BTreeMap<TransactionDigest, CertifiedOrder>,
    /// The known objects with it's sequence number owned by the client.
    object_ids: BTreeMap<ObjectID, SequenceNumber>,
}

// Operations are considered successful when they successfully reach a quorum of authorities.
pub trait Client {
    /// Send money to a FastPay account.
    fn transfer_to_fastpay(
        &mut self,
        object_id: ObjectID,
        recipient: FastPayAddress,
        user_data: UserData,
    ) -> AsyncResult<'_, CertifiedOrder, anyhow::Error>;

    /// Receive money from FastPay.
    fn receive_from_fastpay(
        &mut self,
        certificate: CertifiedOrder,
    ) -> AsyncResult<'_, (), anyhow::Error>;

    /// Send money to a FastPay account.
    /// Do not check balance. (This may block the client)
    /// Do not confirm the transaction.
    fn transfer_to_fastpay_unsafe_unconfirmed(
        &mut self,
        recipient: FastPayAddress,
        object_id: ObjectID,
        user_data: UserData,
    ) -> AsyncResult<'_, CertifiedOrder, anyhow::Error>;

    /// Synchronise client state with a random authorities, updates all object_ids and certificates, request only goes out to one authority.
    /// this method doesn't guarantee data correctness, client will have to handle potential byzantine authority
    fn sync_client_state_with_random_authority(
        &mut self,
    ) -> AsyncResult<'_, AuthorityName, anyhow::Error>;

    /// Get all object we own.
    fn get_owned_objects(&self) -> AsyncResult<'_, Vec<ObjectID>, anyhow::Error>;
}

impl<A> ClientState<A> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        address: FastPayAddress,
        secret: KeyPair,
        committee: Committee,
        authority_clients: HashMap<AuthorityName, A>,
        sent_certificates: Vec<CertifiedOrder>,
        received_certificates: Vec<CertifiedOrder>,
        object_ids: BTreeMap<ObjectID, SequenceNumber>,
    ) -> Self {
        Self {
            address,
            secret,
            committee,
            authority_clients,
            pending_transfer: None,
            sent_certificates,
            received_certificates: received_certificates
                .into_iter()
                .map(|cert| (cert.order.digest(), cert))
                .collect(),
            object_ids,
        }
    }

    pub fn address(&self) -> FastPayAddress {
        self.address
    }

    pub fn next_sequence_number(&self, object_id: ObjectID) -> SequenceNumber {
        self.object_ids[&object_id]
    }

    pub fn object_ids(&self) -> &BTreeMap<ObjectID, SequenceNumber> {
        &self.object_ids
    }

    pub fn pending_transfer(&self) -> &Option<Order> {
        &self.pending_transfer
    }

    pub fn sent_certificates(&self) -> &Vec<CertifiedOrder> {
        &self.sent_certificates
    }

    pub fn received_certificates(&self) -> impl Iterator<Item = &CertifiedOrder> {
        self.received_certificates.values()
    }
}

#[derive(Clone)]
struct CertificateRequester<A> {
    committee: Committee,
    authority_clients: Vec<A>,
    sender: Option<FastPayAddress>,
    object_id: ObjectID,
}

impl<A> CertificateRequester<A> {
    fn new(
        committee: Committee,
        authority_clients: Vec<A>,
        sender: Option<FastPayAddress>,
        object_id: ObjectID,
    ) -> Self {
        Self {
            committee,
            authority_clients,
            sender,
            object_id,
        }
    }
}

impl<A> Requester for CertificateRequester<A>
where
    A: AuthorityClient + Send + Sync + 'static + Clone,
{
    type Key = SequenceNumber;
    type Value = Result<CertifiedOrder, FastPayError>;

    /// Try to find a certificate for the given sender and sequence number.
    fn query(
        &mut self,
        sequence_number: SequenceNumber,
    ) -> AsyncResult<'_, CertifiedOrder, FastPayError> {
        Box::pin(async move {
            let request = ObjectInfoRequest {
                object_id: self.object_id,
                request_sequence_number: Some(sequence_number),
                request_received_transfers_excluding_first_nth: None,
            };
            // Sequentially try each authority in random order.
            // TODO: Improve shuffle, different authorities might different amount of stake.
            self.authority_clients.shuffle(&mut rand::thread_rng());
            for client in self.authority_clients.iter_mut() {
                let result = client.handle_object_info_request(request.clone()).await;
                if let Ok(response) = result {
                    let certificate = response.requested_certificate.unwrap();
                    if certificate.check(&self.committee).is_ok() {
                        let order = &certificate.order;
                        if let Some(sender) = self.sender {
                            if order.sender() == &sender
                                && order.sequence_number() == sequence_number
                            {
                                return Ok(certificate.clone());
                            }
                        } else {
                            return Ok(certificate.clone());
                        }
                    }
                }
            }
            Err(FastPayError::ErrorWhileRequestingCertificate)
        })
    }
}

/// Used for communicate_transfers
#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
enum CommunicateAction {
    SendOrder(Order),
    SynchronizeNextSequenceNumber(SequenceNumber),
}

impl<A> ClientState<A>
where
    A: AuthorityClient + Send + Sync + 'static + Clone,
{
    #[cfg(test)]
    async fn request_certificate(
        &mut self,
        sender: FastPayAddress,
        object_id: ObjectID,
        sequence_number: SequenceNumber,
    ) -> Result<CertifiedOrder, FastPayError> {
        CertificateRequester::new(
            self.committee.clone(),
            self.authority_clients.values().cloned().collect(),
            Some(sender),
            object_id,
        )
        .query(sequence_number)
        .await
    }

    /// Find the highest sequence number that is known to a quorum of authorities.
    /// NOTE: This is only reliable in the synchronous model, with a sufficient timeout value.
    #[cfg(test)]
    async fn get_strong_majority_sequence_number(&self, object_id: ObjectID) -> SequenceNumber {
        let request = ObjectInfoRequest {
            object_id,
            request_sequence_number: None,
            request_received_transfers_excluding_first_nth: None,
        };
        let mut authority_clients = self.authority_clients.clone();
        let numbers: futures::stream::FuturesUnordered<_> = authority_clients
            .iter_mut()
            .map(|(name, client)| {
                let fut = client.handle_object_info_request(request.clone());
                async move {
                    match fut.await {
                        Ok(info) => Some((*name, info.next_sequence_number)),
                        _ => None,
                    }
                }
            })
            .collect();
        self.committee.get_strong_majority_lower_bound(
            numbers.filter_map(|x| async move { x }).collect().await,
        )
    }

    /// Return owner address and sequence number of an object backed by a quorum of authorities.
    /// NOTE: This is only reliable in the synchronous model, with a sufficient timeout value.
    #[cfg(test)]
    async fn get_strong_majority_owner(
        &self,
        object_id: ObjectID,
    ) -> Option<(FastPayAddress, SequenceNumber)> {
        let request = ObjectInfoRequest {
            object_id,
            request_sequence_number: None,
            request_received_transfers_excluding_first_nth: None,
        };
        let authority_clients = self.authority_clients.clone();
        let numbers: futures::stream::FuturesUnordered<_> = authority_clients
            .iter()
            .map(|(name, client)| {
                let fut = client.handle_object_info_request(request.clone());
                async move {
                    match fut.await {
                        Ok(info) => Some((*name, Some((info.owner, info.next_sequence_number)))),
                        _ => None,
                    }
                }
            })
            .collect();
        self.committee.get_strong_majority_lower_bound(
            numbers.filter_map(|x| async move { x }).collect().await,
        )
    }

    /// Execute a sequence of actions in parallel for a quorum of authorities.
    async fn communicate_with_quorum<'a, V, F>(
        &'a mut self,
        execute: F,
    ) -> Result<Vec<V>, anyhow::Error>
    where
        F: Fn(AuthorityName, &'a mut A) -> AsyncResult<'a, V, FastPayError> + Clone,
    {
        let committee = &self.committee;
        let authority_clients = &mut self.authority_clients;
        let mut responses: futures::stream::FuturesUnordered<_> = authority_clients
            .iter_mut()
            .map(|(name, client)| {
                let execute = execute.clone();
                async move { (*name, execute(*name, client).await) }
            })
            .collect();

        let mut values = Vec::new();
        let mut value_score = 0;
        let mut error_scores = HashMap::new();
        while let Some((name, result)) = responses.next().await {
            match result {
                Ok(value) => {
                    values.push(value);
                    value_score += committee.weight(&name);
                    if value_score >= committee.quorum_threshold() {
                        // Success!
                        return Ok(values);
                    }
                }
                Err(err) => {
                    let entry = error_scores.entry(err.clone()).or_insert(0);
                    *entry += committee.weight(&name);
                    if *entry >= committee.validity_threshold() {
                        // At least one honest node returned this error.
                        // No quorum can be reached, so return early.
                        bail!(
                            "Failed to communicate with a quorum of authorities: {}",
                            err
                        );
                    }
                }
            }
        }

        bail!("Failed to communicate with a quorum of authorities (multiple errors)");
    }

    /// Broadcast confirmation orders and optionally one more transfer order.
    /// The corresponding sequence numbers should be consecutive and increasing.
    async fn communicate_transfers(
        &mut self,
        sender: FastPayAddress,
        object_id: ObjectID,
        known_certificates: Vec<CertifiedOrder>,
        action: CommunicateAction,
    ) -> Result<Vec<CertifiedOrder>, anyhow::Error> {
        let target_sequence_number = match &action {
            CommunicateAction::SendOrder(order) => order.sequence_number(),
            CommunicateAction::SynchronizeNextSequenceNumber(seq) => *seq,
        };
        let requester = CertificateRequester::new(
            self.committee.clone(),
            self.authority_clients.values().cloned().collect(),
            Some(sender),
            object_id,
        );
        let (task, mut handle) = Downloader::start(
            requester,
            known_certificates.into_iter().filter_map(|cert| {
                if cert.order.sender() == &sender {
                    Some((cert.order.sequence_number(), Ok(cert)))
                } else {
                    None
                }
            }),
        );
        let committee = self.committee.clone();
        let votes = self
            .communicate_with_quorum(|name, client| {
                let mut handle = handle.clone();
                let action = action.clone();
                let committee = &committee;
                Box::pin(async move {
                    // Figure out which certificates this authority is missing.
                    let request = ObjectInfoRequest {
                        object_id,
                        request_sequence_number: None,
                        request_received_transfers_excluding_first_nth: None,
                    };
                    let response = client.handle_object_info_request(request).await?;

                    let current_sequence_number = response.next_sequence_number;
                    // Download each missing certificate in reverse order using the downloader.
                    let mut missing_certificates = Vec::new();
                    let mut number = target_sequence_number.decrement();
                    while let Ok(value) = number {
                        if value < current_sequence_number {
                            break;
                        }
                        let certificate = handle
                            .query(value)
                            .await
                            .map_err(|_| FastPayError::ErrorWhileRequestingCertificate)??;
                        missing_certificates.push(certificate);
                        number = value.decrement();
                    }
                    // Send all missing confirmation orders.
                    missing_certificates.reverse();
                    for certificate in missing_certificates {
                        client
                            .handle_confirmation_order(ConfirmationOrder::new(certificate))
                            .await?;
                    }
                    // Send the transfer order (if any) and return a vote.
                    if let CommunicateAction::SendOrder(order) = action {
                        let result = client.handle_order(order).await;
                        return match result {
                            Ok(ObjectInfoResponse {
                                pending_confirmation: Some(signed_order),
                                ..
                            }) => {
                                fp_ensure!(
                                    signed_order.authority == name,
                                    FastPayError::ErrorWhileProcessingTransferOrder
                                );
                                signed_order.check(committee)?;
                                Ok(Some(signed_order))
                            }
                            Err(err) => Err(err),
                            _ => Err(FastPayError::ErrorWhileProcessingTransferOrder),
                        };
                    }
                    Ok(None)
                })
            })
            .await?;
        // Terminate downloader task and retrieve the content of the cache.
        handle.stop().await?;
        let mut certificates: Vec<_> = task.await.unwrap().filter_map(Result::ok).collect();
        if let CommunicateAction::SendOrder(order) = action {
            let certificate = CertifiedOrder {
                order,
                signatures: votes
                    .into_iter()
                    .filter_map(|vote| match vote {
                        Some(signed_order) => {
                            Some((signed_order.authority, signed_order.signature))
                        }
                        None => None,
                    })
                    .collect(),
            };
            // Certificate is valid because
            // * `communicate_with_quorum` ensured a sufficient "weight" of (non-error) answers were returned by authorities.
            // * each answer is a vote signed by the expected authority.
            certificates.push(certificate);
        }
        Ok(certificates)
    }

    /// Make sure we have all our certificates with sequence number
    /// in the range 0..self.next_sequence_number
    pub async fn download_certificates(&self) -> Result<Vec<CertifiedOrder>, FastPayError> {
        let mut sent_certificates = Vec::new();

        for (object_id, next_sequence_number) in self.object_ids.clone() {
            let known_sequence_numbers: BTreeSet<_> = self
                .sent_certificates
                .iter()
                .filter(|cert| cert.order.object_id() == &object_id)
                .map(|cert| cert.order.sequence_number())
                .collect();

            let mut requester = CertificateRequester::new(
                self.committee.clone(),
                self.authority_clients.values().cloned().collect(),
                None,
                object_id,
            );

            // TODO: it's inefficient to loop through sequence numbers to retrieve missing cert, rethink this logic when we change certificate storage in client.
            let mut number = SequenceNumber::from(0);
            while number < next_sequence_number {
                if !known_sequence_numbers.contains(&number) {
                    let certificate = requester.query(number).await?;
                    sent_certificates.push(certificate);
                }
                number = number.increment().unwrap_or_else(|_| SequenceNumber::max());
            }
        }

        sent_certificates.sort_by_key(|cert| cert.order.sequence_number());
        Ok(sent_certificates)
    }

    /// Transfers an object to a recipient address.
    async fn transfer(
        &mut self,
        (object_id, sequence_number, _object_digest): ObjectRef,
        recipient: Address,
        user_data: UserData,
    ) -> Result<CertifiedOrder, anyhow::Error> {
        let transfer = Transfer {
            object_ref: (object_id, sequence_number, _object_digest),
            sender: self.address,
            recipient,
            user_data,
        };
        let order = Order::new_transfer(transfer, &self.secret);
        let certificate = self
            .execute_transfer(order, /* with_confirmation */ true)
            .await?;
        Ok(certificate)
    }

    /// Update our view of certificates. Update the object_id and the next sequence number accordingly.
    /// NOTE: This is only useful in the eventuality of missing local data.
    /// We assume certificates to be valid and sent by us, and their sequence numbers to be unique.
    fn update_certificates(
        &mut self,
        certificates: Vec<CertifiedOrder>,
    ) -> Result<(), FastPayError> {
        for new_cert in &certificates {
            let object_id = *new_cert.order.object_id();
            let mut new_next_sequence_number = self.next_sequence_number(object_id);

            if new_cert.order.sequence_number() >= new_next_sequence_number {
                new_next_sequence_number = new_cert
                    .order
                    .sequence_number()
                    .increment()
                    .unwrap_or_else(|_| SequenceNumber::max());
            }
            // store cert in sent_certificate if the cert is created by the client, else store it in received certificates.
            // TODO: Refactor how we stor certs in client to avoid this logic https://github.com/MystenLabs/fastnft/issues/82
            if *new_cert.order.sender() == self.address {
                self.sent_certificates.push(new_cert.clone());
            } else {
                self.received_certificates
                    .insert(new_cert.order.digest(), new_cert.clone());
            }

            // Atomic update
            self.object_ids.insert(object_id, new_next_sequence_number);
            // Sanity check
            // Some certificates of the object will be from received_certs if the object is originated from other sender.
            // TODO: Maybe we should store certificates in one place sorted by object_ref instead of sent/received?
            let mut sent_certificates: Vec<CertifiedOrder> = self
                .sent_certificates
                .iter()
                .filter(|cert| *cert.order.object_id() == object_id)
                .cloned()
                .collect();

            let mut received_certs: Vec<CertifiedOrder> = self
                .received_certificates
                .values()
                .filter(|cert| *cert.order.object_id() == object_id)
                .cloned()
                .collect();

            sent_certificates.append(&mut received_certs);

            assert_eq!(
                sent_certificates.len(),
                usize::from(self.next_sequence_number(object_id))
            );
        }
        Ok(())
    }

    /// Execute (or retry) a transfer order. Update local balance.
    async fn execute_transfer(
        &mut self,
        order: Order,
        with_confirmation: bool,
    ) -> Result<CertifiedOrder, anyhow::Error> {
        ensure!(
            self.pending_transfer == None || self.pending_transfer.as_ref() == Some(&order),
            "Client state has a different pending transfer",
        );
        ensure!(
            order.sequence_number() == self.next_sequence_number(*order.object_id()),
            "Unexpected sequence number"
        );
        self.pending_transfer = Some(order.clone());
        let new_sent_certificates = self
            .communicate_transfers(
                self.address,
                *order.object_id(),
                self.sent_certificates.clone(),
                CommunicateAction::SendOrder(order.clone()),
            )
            .await?;
        assert_eq!(new_sent_certificates.last().unwrap().order, order);
        // Clear `pending_transfer` and update `sent_certificates`,
        // and `next_sequence_number`. (Note that if we were using persistent
        // storage, we should ensure update atomicity in the eventuality of a crash.)
        self.pending_transfer = None;
        self.update_certificates(new_sent_certificates)?;
        // Confirm last transfer certificate if needed.
        if with_confirmation {
            self.communicate_transfers(
                self.address,
                *order.object_id(),
                self.sent_certificates.clone(),
                CommunicateAction::SynchronizeNextSequenceNumber(
                    self.next_sequence_number(*order.object_id()),
                ),
            )
            .await?;
        }
        Ok(self.sent_certificates.last().unwrap().clone())
    }

    async fn download_own_object_ids(
        &self,
    ) -> Result<(AuthorityName, Vec<ObjectRef>), FastPayError> {
        let request = AccountInfoRequest {
            account: self.address,
        };
        // Sequentially try each authority in random order.
        let mut authorities: Vec<AuthorityName> =
            self.authority_clients.clone().into_keys().collect();
        // TODO: implement sampling according to stake distribution and using secure RNG. https://github.com/MystenLabs/fastnft/issues/128
        authorities.shuffle(&mut rand::thread_rng());
        // Authority could be byzantine, add timeout to avoid waiting forever.
        for authority_name in authorities {
            let authority = self.authority_clients.get(&authority_name).unwrap();
            let result = timeout(
                AUTHORITY_REQUEST_TIMEOUT,
                authority.handle_account_info_request(request.clone()),
            )
            .map_err(|_| FastPayError::ErrorWhileRequestingInformation)
            .await?;
            if let Ok(AccountInfoResponse { object_ids, .. }) = &result {
                return Ok((authority_name, object_ids.clone()));
            }
        }
        Err(FastPayError::ErrorWhileRequestingInformation)
    }
}

impl<A> Client for ClientState<A>
where
    A: AuthorityClient + Send + Sync + Clone + 'static,
{
    fn transfer_to_fastpay(
        &mut self,
        object_id: ObjectID,
        recipient: FastPayAddress,
        user_data: UserData,
    ) -> AsyncResult<'_, CertifiedOrder, anyhow::Error> {
        Box::pin(self.transfer(
            (
                object_id,
                self.next_sequence_number(object_id),
                // TODO(https://github.com/MystenLabs/fastnft/issues/123): Include actual object digest here
                ObjectDigest::new([0; 32]),
            ),
            Address::FastPay(recipient),
            user_data,
        ))
    }

    fn receive_from_fastpay(
        &mut self,
        certificate: CertifiedOrder,
    ) -> AsyncResult<'_, (), anyhow::Error> {
        Box::pin(async move {
            certificate.check(&self.committee)?;
            match &certificate.order.kind {
                OrderKind::Transfer(transfer) => {
                    ensure!(
                        transfer.recipient == Address::FastPay(self.address),
                        "Transfer should be received by us."
                    );
                    self.communicate_transfers(
                        transfer.sender,
                        *certificate.order.object_id(),
                        vec![certificate.clone()],
                        CommunicateAction::SynchronizeNextSequenceNumber(
                            transfer.object_ref.1.increment()?,
                        ),
                    )
                    .await?;
                    // Everything worked: update the local balance.
                    if let btree_map::Entry::Vacant(entry) =
                        self.received_certificates.entry(certificate.order.digest())
                    {
                        self.object_ids.insert(
                            transfer.object_ref.0,
                            transfer.object_ref.1.increment().unwrap(),
                        );
                        entry.insert(certificate);
                    }
                    Ok(())
                }
                OrderKind::Publish(_) | OrderKind::Call(_) => {
                    unimplemented!("receiving (?) Move call or publish")
                }
            }
        })
    }

    fn transfer_to_fastpay_unsafe_unconfirmed(
        &mut self,
        recipient: FastPayAddress,
        object_id: ObjectID,
        user_data: UserData,
    ) -> AsyncResult<'_, CertifiedOrder, anyhow::Error> {
        Box::pin(async move {
            let transfer = Transfer {
                object_ref: (
                    object_id,
                    self.next_sequence_number(object_id),
                    // TODO(https://github.com/MystenLabs/fastnft/issues/123): Include actual object digest here
                    ObjectDigest::new([0; 32]),
                ),
                sender: self.address,
                recipient: Address::FastPay(recipient),
                user_data,
            };
            let order = Order::new_transfer(transfer, &self.secret);
            let new_certificate = self
                .execute_transfer(order, /* with_confirmation */ false)
                .await?;
            Ok(new_certificate)
        })
    }

    fn sync_client_state_with_random_authority(
        &mut self,
    ) -> AsyncResult<'_, AuthorityName, anyhow::Error> {
        Box::pin(async move {
            if let Some(order) = self.pending_transfer.clone() {
                // Finish executing the previous transfer.
                self.execute_transfer(order, /* with_confirmation */ false)
                    .await?;
            }
            // update object_ids.
            self.object_ids.clear();

            let (authority_name, object_ids) = self.download_own_object_ids().await?;
            for (object_id, sequence_number, _) in object_ids {
                self.object_ids.insert(object_id, sequence_number);
            }

            // Recover missing certificates.
            let new_certificates = self.download_certificates().await?;
            self.update_certificates(new_certificates)?;

            Ok(authority_name)
        })
    }

    fn get_owned_objects(&self) -> AsyncResult<'_, Vec<ObjectID>, anyhow::Error> {
        Box::pin(async move { Ok(self.object_ids.keys().copied().collect()) })
    }
}
