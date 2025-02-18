// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Primitives of messages module, that are used on the source chain.

use crate::{InboundLaneData, LaneId, MessageNonce, OutboundLaneData};

use crate::UnrewardedRelayer;
use bp_runtime::Size;
use frame_support::{weights::Weight, Parameter, RuntimeDebug};
use sp_std::{
	collections::{btree_map::BTreeMap, vec_deque::VecDeque},
	fmt::Debug,
	ops::RangeInclusive,
};

/// Number of messages, delivered by relayers.
pub type RelayersRewards<AccountId> = BTreeMap<AccountId, MessageNonce>;

/// Target chain API. Used by source chain to verify target chain proofs.
///
/// All implementations of this trait should only work with finalized data that
/// can't change. Wrong implementation may lead to invalid lane states (i.e. lane
/// that's stuck) and/or processing messages without paying fees.
///
/// The `Payload` type here means the payload of the message that is sent from the
/// source chain to the target chain. The `AccountId` type here means the account
/// type used by the source chain.
pub trait TargetHeaderChain<Payload, AccountId> {
	/// Error type.
	type Error: Debug;

	/// Proof that messages have been received by target chain.
	type MessagesDeliveryProof: Parameter + Size;

	/// Verify message payload before we accept it.
	///
	/// **CAUTION**: this is very important function. Incorrect implementation may lead
	/// to stuck lanes and/or relayers loses.
	///
	/// The proper implementation must ensure that the delivery-transaction with this
	/// payload would (at least) be accepted into target chain transaction pool AND
	/// eventually will be successfully mined. The most obvious incorrect implementation
	/// example would be implementation for BTC chain that accepts payloads larger than
	/// 1MB. BTC nodes aren't accepting transactions that are larger than 1MB, so relayer
	/// will be unable to craft valid transaction => this (and all subsequent) messages will
	/// never be delivered.
	fn verify_message(payload: &Payload) -> Result<(), Self::Error>;

	/// Verify messages delivery proof and return lane && nonce of the latest received message.
	fn verify_messages_delivery_proof(
		proof: Self::MessagesDeliveryProof,
	) -> Result<(LaneId, InboundLaneData<AccountId>), Self::Error>;
}

/// Lane message verifier.
///
/// Runtime developer may implement any additional validation logic over message-lane mechanism.
/// E.g. if lanes should have some security (e.g. you can only accept Lane1 messages from
/// Submitter1, Lane2 messages for those who has submitted first message to this lane, disable
/// Lane3 until some block, ...), then it may be built using this verifier.
///
/// Any fee requirements should also be enforced here.
pub trait LaneMessageVerifier<SenderOrigin, Payload> {
	/// Error type.
	type Error: Debug + Into<&'static str>;

	/// Verify message payload and return Ok(()) if message is valid and allowed to be sent over the
	/// lane.
	fn verify_message(
		submitter: &SenderOrigin,
		lane: &LaneId,
		outbound_data: &OutboundLaneData,
		payload: &Payload,
	) -> Result<(), Self::Error>;
}

/// Manages payments that are happening at the source chain during delivery confirmation
/// transaction.
pub trait DeliveryConfirmationPayments<AccountId> {
	/// Error type.
	type Error: Debug + Into<&'static str>;

	/// Pay rewards for delivering messages to the given relayers.
	///
	/// The implementation may also choose to pay reward to the `confirmation_relayer`, which is
	/// a relayer that has submitted delivery confirmation transaction.
	fn pay_reward(
		lane_id: LaneId,
		messages_relayers: VecDeque<UnrewardedRelayer<AccountId>>,
		confirmation_relayer: &AccountId,
		received_range: &RangeInclusive<MessageNonce>,
	);
}

impl<AccountId> DeliveryConfirmationPayments<AccountId> for () {
	type Error = &'static str;

	fn pay_reward(
		_lane_id: LaneId,
		_messages_relayers: VecDeque<UnrewardedRelayer<AccountId>>,
		_confirmation_relayer: &AccountId,
		_received_range: &RangeInclusive<MessageNonce>,
	) {
		// this implementation is not rewarding relayers at all
	}
}

/// Send message artifacts.
#[derive(Eq, RuntimeDebug, PartialEq)]
pub struct SendMessageArtifacts {
	/// Nonce of the message.
	pub nonce: MessageNonce,
	/// Actual weight of send message call.
	pub weight: Weight,
}

/// Messages bridge API to be used from other pallets.
pub trait MessagesBridge<SenderOrigin, Payload> {
	/// Error type.
	type Error: Debug;

	/// Send message over the bridge.
	///
	/// Returns unique message nonce or error if send has failed.
	fn send_message(
		sender: SenderOrigin,
		lane: LaneId,
		message: Payload,
	) -> Result<SendMessageArtifacts, Self::Error>;
}

/// Bridge that does nothing when message is being sent.
#[derive(Eq, RuntimeDebug, PartialEq)]
pub struct NoopMessagesBridge;

impl<SenderOrigin, Payload> MessagesBridge<SenderOrigin, Payload> for NoopMessagesBridge {
	type Error = &'static str;

	fn send_message(
		_sender: SenderOrigin,
		_lane: LaneId,
		_message: Payload,
	) -> Result<SendMessageArtifacts, Self::Error> {
		Ok(SendMessageArtifacts { nonce: 0, weight: Weight::zero() })
	}
}

/// Structure that may be used in place of `TargetHeaderChain`, `LaneMessageVerifier` and
/// `MessageDeliveryAndDispatchPayment` on chains, where outbound messages are forbidden.
pub struct ForbidOutboundMessages;

/// Error message that is used in `ForbidOutboundMessages` implementation.
const ALL_OUTBOUND_MESSAGES_REJECTED: &str =
	"This chain is configured to reject all outbound messages";

impl<Payload, AccountId> TargetHeaderChain<Payload, AccountId> for ForbidOutboundMessages {
	type Error = &'static str;

	type MessagesDeliveryProof = ();

	fn verify_message(_payload: &Payload) -> Result<(), Self::Error> {
		Err(ALL_OUTBOUND_MESSAGES_REJECTED)
	}

	fn verify_messages_delivery_proof(
		_proof: Self::MessagesDeliveryProof,
	) -> Result<(LaneId, InboundLaneData<AccountId>), Self::Error> {
		Err(ALL_OUTBOUND_MESSAGES_REJECTED)
	}
}

impl<SenderOrigin, Payload> LaneMessageVerifier<SenderOrigin, Payload> for ForbidOutboundMessages {
	type Error = &'static str;

	fn verify_message(
		_submitter: &SenderOrigin,
		_lane: &LaneId,
		_outbound_data: &OutboundLaneData,
		_payload: &Payload,
	) -> Result<(), Self::Error> {
		Err(ALL_OUTBOUND_MESSAGES_REJECTED)
	}
}

impl<AccountId> DeliveryConfirmationPayments<AccountId> for ForbidOutboundMessages {
	type Error = &'static str;

	fn pay_reward(
		_lane_id: LaneId,
		_messages_relayers: VecDeque<UnrewardedRelayer<AccountId>>,
		_confirmation_relayer: &AccountId,
		_received_range: &RangeInclusive<MessageNonce>,
	) {
	}
}
