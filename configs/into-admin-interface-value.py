# SPDX-FileCopyrightText: Copyright 2022-2024 EDF (Électricité de France S.A.)
# SPDX-License-Identifier: BSD-3-Clause
# See README for all details on copyright, authorship and license.
"""
Script that encodes the devices' keys as needed to update a keycloak instance through the admin GUI.

The produced value can be entered in the client's Authorization / Resources
tab, on the resource named "ace-oauth-key-value-store", in the resource
attribute "ec2-p256-public-key". Beware that the format used there is internal
to the keycloak extension, and may be changed.
"""

import glob
import yaml
import base64
from ecdsa import NIST256p
from ecdsa.ellipticcurve import Point

for f in glob.glob("*.yaml"):
    config = yaml.load(open(f), yaml.SafeLoader)
    if not "edhoc_x" in config:
        print(f"{config['audience']} has no public key configured.")
        continue

    x = int.from_bytes(bytes.fromhex(config["edhoc_x"]), "big")
    y = int.from_bytes(bytes.fromhex(config["edhoc_y"]), "big")

    point = Point(NIST256p.curve, x, y)
    compressed = point.to_bytes(encoding="compressed")
    encoded = base64.b64encode(compressed).decode("ascii")

    print(f"{config['audience']}: {encoded}")
