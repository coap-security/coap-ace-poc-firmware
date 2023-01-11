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

for i in range(10):
    d = {
        'issuer': "AS",
        'as_uri': "https://as.coap.amsuess.com/token",
        }
    d['audience'] = "d%02d" % i
    d['key'] = secrets.token_bytes(32).hex()
    with open("%s.yaml" % d['audience'], "w") as o:
        yaml.dump(d, o)
