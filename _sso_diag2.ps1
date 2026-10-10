# 1) device session
$session = curl.exe -s -m 10 -X POST "https://shop.greenpool.cn/api/session" -H "Content-Type: application/json" -d '{\"device_id\":\"11111111-2222-3333-4444-555555555555\",\"app_version\":\"0.1.5\"}' | ConvertFrom-Json
$st = $session.data.session_token
"session_token len=$($st.Length)"

# 2) login body via file (avoid PS quoting hell)
$body = "{`"email`":`"987321288@qq.com`",`"password`":`"Bb710095377*`",`"session_token`":`"$st`"}"
[IO.File]::WriteAllText("$env:TEMP\shop_login.json", $body)
$login = curl.exe -s -m 10 -X POST "https://shop.greenpool.cn/api/user/login" -H "Content-Type: application/json" --data-binary "@$env:TEMP\shop_login.json"
$lo = $login | ConvertFrom-Json
"login success=$($lo.success) msg=$($lo.message)"
$auth = $lo.data.session_token
"auth len=$($auth.Length)"
if ($auth.Length -gt 10) {
  # 3) SSO with real token
  "=== SSO headers ==="
  curl.exe -s -m 10 -D - -o NUL "https://shop.greenpool.cn/api/sso?session_token=$auth" | Select-String -Pattern 'HTTP/|Location|Set-Cookie' | Select-Object -First 6
}
