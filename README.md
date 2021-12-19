# octopus-pallets
Collection of pallets used in Octopus Network

# pallets

## [pallet-octopus-appchain](https://github.com/octopus-network/octopus-pallets/tree/main/appchain)
- Metadata of an appchain. Including appchain identifier, RPC endpoint of mainchain, etc.
- Validators of the appchain will observe the mainchain and submit the observed events using OCW for consensus.

## [pallet-ibc](https://github.com/octopus-network/octopus-pallets/tree/main/ibc)
- This pallet implements the standard [IBC protocol](https://github.com/cosmos/ics).

## [pallet-octopus-lpos](https://github.com/octopus-network/octopus-pallets/tree/main/lpos)
- An implementation of Octopus Network's LPoS.
- This pallet depends on pallet-octopus-appchain.

## [pallet-octopus-support](https://github.com/octopus-network/octopus-pallets/tree/main/support)
- Some common traits and types.

## [pallet-octopus-upward-messages](https://github.com/octopus-network/octopus-pallets/tree/main/upward-messages)
- This pallet manages the cross-chain messages sent from appchain to mainchain.
