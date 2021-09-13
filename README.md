# octopus-pallets
Collection of pallets used in Octopus Network

# pallets

## [pallet-octopus-appchain](https://github.com/octopus-network/octopus-pallets/tree/main/appchain)
- metadata of an appchain, including appchain identifier, RPC endpoint of mainchain, etc.
- validators of appchain will observe the mainchain and submit the observed events using OCW for consensus. 

## [pallet-octopus-lpos](https://github.com/octopus-network/octopus-pallets/tree/main/lpos)
- an implementation of Octopus Network's LPoS.
- this pallet depends on pallet-octopus-appchain.

## [pallet-ibc](https://github.com/octopus-network/octopus-pallets/tree/main/ibc)
- this pallet is currently in [substrate-ibc](https://github.com/octopus-network/substrate-ibc) and will be moved here after refactoring.
