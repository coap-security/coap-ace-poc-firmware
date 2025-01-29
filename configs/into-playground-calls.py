# SPDX-FileCopyrightText: Copyright 2022-2024 EDF (Électricité de France S.A.)
# SPDX-License-Identifier: BSD-3-Clause
# See README for all details on copyright, authorship and license.
"""
Script that builds a playground configuration script from the .yaml files in
this directory.

This needs suitable admin credentials for the configured devices' AS to be
present in the KEYCLOAK_ADMINUSER and KEYCLOAK_ADMINPASSWORD environment
variables.
"""

import glob
import yaml
import cbor2
import base64

for f in glob.glob("*.yaml"):
    config = yaml.load(open(f), yaml.SafeLoader)
    realm_base = config['as_uri'].removesuffix("/ace-oauth/token")
    if realm_base == config['as_uri']:
        raise RuntimeError("as_uri does not end with the expected /ace-oauth/token suffix")
    base_url, _, realm = realm_base.partition("/realms/")
    if not realm:
        raise RuntimeError("as_uri does not use the expected base-realm split <${BASE}/realms/${REALM}/ace-oauth/token>")

    if 'edhoc_x' in config:
        # This is somewhat funny back-and-forth encoding, but it is easier to
        # maintain than duplicating the create-resource-server-in-realm.py
        # script
        x = bytes.fromhex(config['edhoc_x'])
        y = bytes.fromhex(config['edhoc_y'])
        ccs = {8: {1: {1: 2, -1: 1, -2: x, -3: y}}}
        p256b64 = base64.b64encode(cbor2.dumps(ccs, canonical=True)).decode('ascii')
    else:
        p256b64 = None

    print(f"create-resource-server-in-realm.py --identifier {config['audience']} --realm {realm} {'--p256-public-key ' + p256b64 if p256b64 else ''} {base_url} --admin-base-url {base_url}:8443")
