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
