import ctypes, json, uuid, urllib.request, urllib.error
from ctypes import wintypes

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

email = cred_read('newapi_email.cn.greenpool.gp')
pwd = cred_read('newapi_password.cn.greenpool.gp')
BASE = 'https://shop.greenpool.cn'
UA = 'GreenPool/0.1.5'

def req(method, path, body=None):
    r = urllib.request.Request(BASE + path, method=method)
    r.add_header('Content-Type', 'application/json')
    r.add_header('User-Agent', UA)
    data = json.dumps(body).encode() if body is not None else None
    try:
        resp = urllib.request.urlopen(r, data, timeout=15)
        return resp.status, dict(resp.headers), resp.read()
    except urllib.error.HTTPError as e:
        return e.code, dict(e.headers), e.read()

_, _, body = req('POST', '/api/session', {'device_id': str(uuid.uuid4()), 'app_version': '0.1.5'})
st = json.loads(body)['data']['session_token']
code, _, body = req('POST', '/api/user/login', {'email': email, 'password': pwd, 'session_token': st})
auth = json.loads(body)['data']['session_token']

code, headers, body = req('GET', f'/api/sso?session_token={auth}')
print('SSO status:', code)
print('body preview:', body[:400].decode('utf-8', errors='replace'))
