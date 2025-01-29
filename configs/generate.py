# SPDX-FileCopyrightText: Copyright 2022-2024 EDF (Électricité de France S.A.)
# SPDX-License-Identifier: BSD-3-Clause
# See README for all details on copyright, authorship and license.
"""
Script that generated the .yaml files in this directory

This is not run automatically because it would generate different keys each
time, which would impede the operation of a demo. Outside of a demo context,
these keys would be generated at commissioning time, and stored in the AS
(which may also generate them).

When regenerating the keys or adding any, make sure to update the copy of the
keys that the AS is using.
"""
import secrets
import yaml

DEVICES = 20
DEVICES_WITH_STATIC_KEYS = 10

for i in range(20):
    d = {
        'issuer': "AS",
        'as_uri': "https://keycloak.coap.amsuess.com/realms/edf/ace-oauth/token",
        }
    d['audience'] = "d%02d" % i
    if i < DEVICES_WITH_STATIC_KEYS:
        d['key'] = secrets.token_bytes(32).hex()

    from cryptography.hazmat.primitives.asymmetric import ec

    private = ec.generate_private_key(curve=ec.SECP256R1())
    public = private.public_key()

    d['edhoc_q'] = private.private_numbers().private_value.to_bytes(32, "big").hex()
    d['edhoc_x'] = public.public_numbers().x.to_bytes(32, "big").hex()
    d['edhoc_y'] = public.public_numbers().y.to_bytes(32, "big").hex()

    # obtained from https://keycloak.coap.amsuess.com/realms/edf/ace-oauth/server-public-keys
    d['as_pub_x'] = "b4108ad8f21d08a877627aaf3787a91afe75a9886e3bffeb152f9fa42c1dfb50"
    d['as_pub_y'] = "6765776379ee0a507e173841669c33fc587bbeac4609b86dfb12af28118baf8a"
    with open("%s.yaml" % d['audience'], "w") as o:
        o.write('''# SPDX-FileCopyrightText: Copyright 2022-2024 EDF (Électricité de France S.A.)
# SPDX-License-Identifier: BSD-3-Clause
# See README for all details on copyright, authorship and license.
''')
        yaml.dump(d, o)
