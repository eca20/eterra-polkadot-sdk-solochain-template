#!/usr/bin/env python3
"""Keep private-alpha credential values out of coordinator children.

The deployment lanes load credentials from hash-pinned environment files at
the narrow component-driver boundary.  Coordinators never own those values, so
an ambient/exported copy is always unauthorized and must not cross a child
process boundary.
"""

from __future__ import annotations

import os
from collections.abc import Mapping, MutableMapping


DEPLOYMENT_SECRET_ENVIRONMENT_NAMES = frozenset(
    {
        "DEPLOY_PASSWORD",
        "REMOTE_SUDO_PASSWORD",
        "AURA_SURI",
        "GRAN_SURI",
        "MEDIA_SIGNER_SEED",
        "MEDIA_ADMIN_API_KEY",
        "AUTHORITY_RELAY_MNEMONIC",
        "AUTHORITY_RELAY_DERIVATION_PASSWORD",
        "ETERRA_LEGENDS_SIGNER_MNEMONIC",
        "ETERRA_LEGENDS_SIGNER_DERIVATION_PASSWORD",
        "ETERRA_LEGENDS_PRIVATE_ALPHA_ACCESS_KEY",
        "ETERRA_ALPHA_SUDO_MNEMONIC",
        "ETERRA_ALPHA_SUDO_DERIVATION_PASSWORD",
        "ADMIN_SESSION_SECRET",
        "ALPHA_ACCESS_SESSION_SECRET",
        "DISCORD_CLIENT_SECRET",
        "DISCORD_BOT_TOKEN",
        "MONGODB_URI",
        "ETERRA_LEGENDS_PLAYER_ACCESS_TOKEN",
        "NEXUS_V2_PRIVATE_ALPHA_ACCESS_KEY",
        "NEXUS_V2_SESSION_AUTHORIZATION_PROFILES_JSON",
        "ADMIN_API_KEY",
        "ETERRA_FPS_V2_OWNER_SECRET_PATH",
        "ETERRA_FPS_V2_PLAYER_GATEWAY_ACCESS_TOKEN",
        "ETERRA_FPS_V2_ROOT_SECRET_PATH",
        "ETERRA_FPS_V2_SUDO_SECRET_PATH",
    }
)


def scrub_environment(environment: MutableMapping[str, str]) -> MutableMapping[str, str]:
    """Remove the closed secret-name union from an environment in place."""

    for name in DEPLOYMENT_SECRET_ENVIRONMENT_NAMES:
        environment.pop(name, None)
    return environment


def scrub_current_process_environment() -> None:
    """Remove inherited deployment secrets before the first subprocess."""

    scrub_environment(os.environ)


def child_environment(
    overrides: Mapping[str, str] | None = None,
) -> dict[str, str]:
    """Return a scrubbed child environment with explicit non-secret overrides."""

    environment = dict(os.environ)
    scrub_environment(environment)
    if overrides:
        overlap = DEPLOYMENT_SECRET_ENVIRONMENT_NAMES.intersection(overrides)
        if overlap:
            names = ", ".join(sorted(overlap))
            raise ValueError(f"secret environment overrides are forbidden: {names}")
        environment.update(overrides)
    return environment


# Importing this guard is the first local action of every active coordinator.
# Scrub once immediately; child_environment() repeats the boundary defensively.
scrub_current_process_environment()
