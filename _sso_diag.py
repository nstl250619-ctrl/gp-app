import json, keyring, uuid, urllib.request, urllib.error

SERVICE = 'cn.greenpool.gp'
BASE = 'https://shop.greenpool.cn'
UA = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) GreenPool/0.1.5'

def post(path, body, headers=None):
    req = urllib.request.Request(BASE + path, method='POST')
    req.add_header('Content-Type', 'application/json')
    req.add_header('User-Agent', UA)
    for k, v in (headers or {}).items():
        req.add_header(k, v)
    resp = urllib.request.urlopen(req, json.dumps(body).encode(), timeout=15)
    return json.loads(resp.read())

def get(path, headers=None):
    req = urllib.request.Request(BASE + path)
    req.add_header('User-Agent', UA)
    for k, v in (headers or {}).items():
        req.add_header(k, v)
    try:
        resp = urllib.request.urlopen(req, timeout=15)
        return resp.status, dict(resp.headers), resp.read()[:200]
    except urllib.error.HTTPError as e:
        return e.code, dict(e.headers), b''

# keyring 枚举可能的键名组合
for user in ['shop_password', 'newapi_password', 'shop_email', 'newapi_email', 'newapi_username']:
    v = keyring.get_password(SERVICE, user)
    print(f'keyring[{user}]:', 'FOUND len=' + str(len(v)) if v else None)

email = keyring.get_password(SERVICE, 'newapi_email') or keyring.get_password(SERVICE, 'shop_email')
pwd = keyring.get_password(SERVICE, 'shop_password') or keyring.get_password(SERVICE, 'newapi_password')
if not email or not pwd:
    print('CREDENTIALS_NOT_FOUND')
    raise SystemExit

# 1) device session
ds = post('/api/session', {'device_id': str(uuid.uuid4()), 'app_version': '0.1.5'})
print('device session:', ds.get('success'), 'token len:', len(ds.get('data', {}).get('session_token', '')))
st = ds['data']['session_token']

# 2) login
lg = post('/api/user/login', {'email': email, 'password': pwd, 'session_token': st})
print('login success:', lg.get('success'), 'msg:', lg.get('message', '')[:80])
auth = lg['data']['session_token']
print('auth token len:', len(auth))

# 3) SSO with real token
code, headers, body = get(f'/api/sso?session_token={auth}')
print('SSO status:', code)
print('SSO Location:', headers.get('Location'))
sc = headers.get('Set-Cookie')
print('SSO Set-Cookie:', (sc[:200] + '...') if sc and len(sc) > 200 else sc)
