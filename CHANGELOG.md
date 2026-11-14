# Changelog

## [0.2.0](https://github.com/Thornolf/discombobulator/compare/v0.1.0...v0.2.0) (2026-11-14)


### Features

* move between scenes with Ctrl+arrows and from the phone ([#62](https://github.com/Thornolf/discombobulator/issues/62)) ([aab835e](https://github.com/Thornolf/discombobulator/commit/aab835efa3c7f73569b3674d23608e32eef2afc7))


### Bug Fixes

* core follow-ups (unreadable folders, local time, render pool, decisions, duplicates, one JPEG decoder) ([#54](https://github.com/Thornolf/discombobulator/issues/54)) ([2efed36](https://github.com/Thornolf/discombobulator/commit/2efed365e92ef7cd4579a973272144c618eb2d6c))
* server and web follow-ups (tailnet scope, live sync, phone, keys, audit) ([#55](https://github.com/Thornolf/discombobulator/issues/55)) ([9088dd6](https://github.com/Thornolf/discombobulator/commit/9088dd69ec2e31440bc2fd67f2d33c90bf5474ed))

## [0.1.0](https://github.com/Thornolf/discombobulator/compare/v0.1.0...v0.1.0) (2026-11-14)


### Features

* **core:** scan, RAF preview extraction, cache and benchmark (M1) ([fbee185](https://github.com/Thornolf/discombobulator/commit/fbee18570a7582281046e6cdd34c738887bb38a5))
* **core:** scan, RAF preview extraction, thumbnail and preview cache, benchmark ([d3d8740](https://github.com/Thornolf/discombobulator/commit/d3d8740953b035aa36155904db11425e211034be)), closes [#3](https://github.com/Thornolf/discombobulator/issues/3)
* desktop Stacks UI, HTTP API and Tauri shell ([40ddb12](https://github.com/Thornolf/discombobulator/commit/40ddb129f065020b4fee17f37597e0beddedb434)), closes [#4](https://github.com/Thornolf/discombobulator/issues/4)
* desktop Stacks UI, HTTP API and Tauri shell (M2) ([fb8253a](https://github.com/Thornolf/discombobulator/commit/fb8253a1ddf13ee72059714a19abf3956199a4c6))
* **desktop:** polish the Windows installer and document install and use ([6f2e22e](https://github.com/Thornolf/discombobulator/commit/6f2e22e3adb25a99c34608b1f6746a013152237f)), closes [#8](https://github.com/Thornolf/discombobulator/issues/8)
* **desktop:** Windows installer polish and user docs (M6) ([02ff1b5](https://github.com/Thornolf/discombobulator/commit/02ff1b51a7bcb23563cf88f6b4d100c858c8a5f0))
* empty _to_delete into the Recycle Bin ([568a249](https://github.com/Thornolf/discombobulator/commit/568a249c313a98a89c4b91fef0b86e67997ba005))
* empty _to_delete into the Recycle Bin from the Apply dialog ([e7232de](https://github.com/Thornolf/discombobulator/commit/e7232de99d42342ff91ba14e151ddfabb6e8f77f)), closes [#42](https://github.com/Thornolf/discombobulator/issues/42)
* exact and similar duplicates ([236546e](https://github.com/Thornolf/discombobulator/commit/236546e07f30c6b8a190a4b240748065ddcbc312)), closes [#6](https://github.com/Thornolf/discombobulator/issues/6)
* exact and similar duplicates (M4) ([e608f05](https://github.com/Thornolf/discombobulator/commit/e608f0532a8ddda81044ff9168a5fe77d82e9a6b))
* phone Swipe PWA over Tailscale with offline mode ([a805371](https://github.com/Thornolf/discombobulator/commit/a805371c9c63dd82c6846981ca84db63f91af5ab)), closes [#7](https://github.com/Thornolf/discombobulator/issues/7)
* phone Swipe PWA over Tailscale with offline mode (M5) ([1047658](https://github.com/Thornolf/discombobulator/commit/1047658a4fdc65df47fb709d6a27ddca44233832))
* XMP sidecars and Apply ([2182ccf](https://github.com/Thornolf/discombobulator/commit/2182ccf14e485758219c14ad230aeeeed9f333e9)), closes [#5](https://github.com/Thornolf/discombobulator/issues/5)
* XMP sidecars and Apply (M3) ([343d3a5](https://github.com/Thornolf/discombobulator/commit/343d3a531aa1a4e428de8d331928d4e99d2d15aa))


### Bug Fixes

* **core:** address M1 review findings ([cd855cf](https://github.com/Thornolf/discombobulator/commit/cd855cf35ca64e792f871bbd82758202b3a71f81))
* **core:** keep decisions when a folder is unreachable and make photo ids stable ([1d69da5](https://github.com/Thornolf/discombobulator/commit/1d69da571f77797fcb172bb1bcd46f65685f0e3e))
* **core:** make sidecar merges and Apply moves safe ([ef4c1e2](https://github.com/Thornolf/discombobulator/commit/ef4c1e2e7ae432a55c04b085866352e3f4b490dd))
* **core:** skip pruning only under folders that could not be read ([27dcae4](https://github.com/Thornolf/discombobulator/commit/27dcae47e23c81c91d01abe813331f8c61acbfb3))
* **desktop:** regenerate icons as 8-bit images ([6053891](https://github.com/Thornolf/discombobulator/commit/60538918c8f7ad844b24dde0828bcbba975ac8be))
* **duplicates:** group around a seed, keep decided photos and debounce the slider ([b8f6cb9](https://github.com/Thornolf/discombobulator/commit/b8f6cb9acd4805a9c9d1086f2c1a9bef28772375))
* **duplicates:** undo works while the tolerance slider has focus ([c85ee06](https://github.com/Thornolf/discombobulator/commit/c85ee063d12c3df4a7eb01ae99e57250670c72b0))
* keep fresh marks across refreshes and stop the old folder's work ([531d183](https://github.com/Thornolf/discombobulator/commit/531d183a848e5afee3f5ff703e81221e3fb2dfb3))
* **phone:** harden Tailscale setup, offline mode and swipes ([3c865d3](https://github.com/Thornolf/discombobulator/commit/3c865d31de567d1751af464d3fa8a199735177e3))
* **phone:** send the app header when saving the photo list offline ([e4e6b0a](https://github.com/Thornolf/discombobulator/commit/e4e6b0aceda8c124e7524b90e6c80f4af77d5c8a))
* **phone:** tag marks with the folder on screen and keep the known tailnet name ([3df70ca](https://github.com/Thornolf/discombobulator/commit/3df70ca8c6c93c34a1fc1245d5c454c1e35dd702))
* **recycle:** honor the no-Recycle-Bin policy and refuse batches larger than the bin ([5622ff4](https://github.com/Thornolf/discombobulator/commit/5622ff49ed22e3b4a342fe63bd227fc3ebe740a1))
* **recycle:** never let Windows erase files that cannot be recycled ([e22c304](https://github.com/Thornolf/discombobulator/commit/e22c304aec27170eee4fd74a3fabe0a0fe309658))
* **server:** require the app header on every API call and refuse network paths ([16e6d8d](https://github.com/Thornolf/discombobulator/commit/16e6d8d32cc4611a1eb84d83280c478ffdef74ce))
* **web:** protect unsynced marks during refresh and wait for in-flight syncs ([c5031e6](https://github.com/Thornolf/discombobulator/commit/c5031e6689278ec6bbb0bd06a556b97dce4757ea))
