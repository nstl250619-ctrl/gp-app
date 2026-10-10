import ctypes, json, uuid, urllib.request
from ctypes import wintypes

TARGETS = {
    'newapi_password.cn.greenpool.gp': 'newapi_password',
    'newapi_email.cn.greenpool.gp': 'newapi_email',
    'shop_password.cn.greenpool.gp': 'shop_password',
}

def cred_read(target):
    adv = ctypes.windll.advapi32
    CredReadW = adv.CredReadW
    CredReadW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
                          ctypes.POINTER(ctypes.c_void_p), ctypes.POINTER(wintypes.DWORD)]
    cred = ctypes.c_void_p()
    count = wintypes.DWORD()
    if not CredReadW(target, 1, 0, ctypes.byref(cred), ctypes.byref(count)):
        return None
    class CREDENTIAL(ctypes.Structure):
        _fields_ = [('Flags', wintypes.DWORD), ('Type', wintypes.DWORD),
                    ('TargetName', wintypes.LPWSTR), ('Comment', wintypes.LPWSTR),
                    ('LastWritten', wintypes.FILETIME), ('CredentialBlobSize', wintypes.DWORD),
                    ('CredentialBlob', ctypes.POINTER(ctypes.c_byte)), ('Persist', wintypes.DWORD),
                    ('AttributeCount', wintypes.DWORD), ('Attributes', ctypes.c_void_p),
                    ('TargetAlias', wintypes.LPWSTR), ('UserName', wintypes.LPWSTR)]
    c = ctypes.cast(cred, ctypes.POINTER(CREDENTIAL)).contents
    blob = ctypes.string_at(c.CredentialBlob, c.CredentialBlobSize) if c.CredentialBlobSize else b''
    ctypes.windll.advapi32.CredFree(cred)
    return blob.decode('utf-16-le') if blob else ''

vals = {k: cred_read(t) for t, k in TARGETS.items()}
for k, v in vals.items():
    print(k, '=', (v[:4] + '...' if v else None))

BASE = 'https://shop.greenpool.cn'
UA = 'GreenPool/0.1.5'
email = vals['newapi_email']
pwd = vals['newapi_password'] or vals['shop_password']

def req(method, path, body=None, headers=None):
    r = urllib.request.Request(BASE + path, method=method)
    r.add_header('Content-Type', 'application/json')
    r.add_header('User-Agent', UA)
    for k, v in (headers or {}).items():
        r.add_header(k, v)
    data = json.dumps(body).encode() if body is not None else None
    try:
        resp = urllib.request.urlopen(r, data, timeout=15)
        return resp.status, dict(resp.headers), resp.read()
    except urllib.error.HTTPError as e:
        return e.code, dict(e.headers), e.read()

code, _, body = req('POST', '/api/session', {'device_id': str(uuid.uuid4()), 'app_version': '0.1.5'})
st = json.loads(body)['data']['session_token']
print('device session:', code, 'len:', len(st))

code, _, body = req('POST', '/api/user/login', {'email': email, 'password': pwd, 'session_token': st})
lg = json.loads(body)
print('login:', code, 'success:', lg.get('success'), 'msg:', str(lg.get('message'))[:60])
auth = lg.get('data', {}).get('session_token', '')
print('auth token len:', len(auth))

if auth:
    code, headers, _ = req('GET', f'/api/sso?session_token={auth}')
    print('SSO:', code, '| Location:', headers.get('Location'))
    sc = headers.get('Set-Cookie')
    print('SSO Set-Cookie gp_token:', 'YES' if sc and 'gp_token' in sc else sc)
