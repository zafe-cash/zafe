from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class Files:
    patterns: tuple[str, ...]

    def __init__(self, *patterns: str):
        object.__setattr__(self, "patterns", patterns)


@dataclass(frozen=True)
class Symbols:
    file: str
    anchors: tuple[str, ...]

    def __init__(self, file: str, *anchors: str):
        object.__setattr__(self, "file", file)
        object.__setattr__(self, "anchors", anchors)


@dataclass(frozen=True)
class Screen:
    route: str
    landmark: str


@dataclass(frozen=True)
class App:
    route: str
    taps: tuple[str, ...] = ()


@dataclass(frozen=True)
class Label:
    text: str


@dataclass(frozen=True)
class Cli:
    command: str


@dataclass(frozen=True)
class RelayRoute:
    path: str


@dataclass(frozen=True)
class Auto:
    when: str


@dataclass(frozen=True)
class Rust:
    crate: str
    target: str | None
    names: tuple[str, ...]
    needs: str | None = None

    def __init__(self, crate: str, target: str | None, *names: str, needs: str | None = None):
        object.__setattr__(self, "crate", crate)
        object.__setattr__(self, "target", target)
        object.__setattr__(self, "names", names)
        object.__setattr__(self, "needs", needs)


@dataclass(frozen=True)
class Flutter:
    file: str


@dataclass(frozen=True)
class Script:
    path: str
    needs: str | None = None


NEEDS = {
    "regtest": "Docker and ths (infra/regtest)",
    "network": "the public internet",
    "frost-tools": "ZCASH_SIGN_BIN, frost-tools zcash-sign",
    "frost-zec": "FROST_ZEC_BIN, OpenPGP-Wallet frost-zec",
    "slow": "minutes of fuzzing",
}


@dataclass(frozen=True)
class Feature:
    id: str
    name: str
    owns: tuple
    reach: tuple
    proofs: tuple
    flows: tuple[str, ...]


@dataclass(frozen=True)
class Area:
    id: str
    name: str
    features: tuple[Feature, ...]


@dataclass(frozen=True)
class Plumbing:
    name: str
    owns: tuple
    proofs: tuple = ()
