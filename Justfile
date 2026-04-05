arrivals:
  curl 'https://transit.ttc.com.ge/pis-gateway/api/v2/stops/1:1353/arrival-times?locale=en&ignoreScheduledArrivalTimes=false' \
  --compressed \
  -H 'User-Agent: Mozilla/5.0 (X11; Linux x86_64; rv:149.0) Gecko/20100101 Firefox/149.0' \
  -H 'Accept: application/json, text/plain, */*' \
  -H 'Accept-Language: en-US,en;q=0.9' \
  -H 'Accept-Encoding: gzip, deflate, br, zstd' \
  -H 'X-api-key: c0a2f304-551a-4d08-b8df-2c53ecd57f9f' \
  -H 'DNT: 1' \
  -H 'Alt-Used: transit.ttc.com.ge' \
  -H 'Connection: keep-alive' \
  -H 'Referer: https://transit.ttc.com.ge/' \
  -H 'Cookie: cookiesession1=678A3E12D36DEB740FA562CFD52AD7AD' \
  -H 'Sec-Fetch-Dest: empty' \
  -H 'Sec-Fetch-Mode: cors' \
  -H 'Sec-Fetch-Site: same-origin' \
  -H 'Sec-GPC: 1' \
  -H 'Pragma: no-cache' \
  -H 'Cache-Control: no-cache'

routes:
  curl 'https://transit.ttc.com.ge/pis-gateway/api/v3/routes?modes=BUS,SUBWAY,GONDOLA&locale=en' \
  --compressed \
  -H 'User-Agent: Mozilla/5.0 (X11; Linux x86_64; rv:149.0) Gecko/20100101 Firefox/149.0' \
  -H 'Accept: application/json, text/plain, */*' \
  -H 'Accept-Language: en-US,en;q=0.9' \
  -H 'Accept-Encoding: gzip, deflate, br, zstd' \
  -H 'X-api-key: c0a2f304-551a-4d08-b8df-2c53ecd57f9f' \
  -H 'DNT: 1' \
  -H 'Alt-Used: transit.ttc.com.ge' \
  -H 'Connection: keep-alive' \
  -H 'Referer: https://transit.ttc.com.ge/' \
  -H 'Cookie: cookiesession1=678A3E12D36DEB740FA562CFD52AD7AD' \
  -H 'Sec-Fetch-Dest: empty' \
  -H 'Sec-Fetch-Mode: cors' \
  -H 'Sec-Fetch-Site: same-origin' \
  -H 'Sec-GPC: 1' \
  -H 'Pragma: no-cache' \
  -H 'Cache-Control: no-cache' \
  -H 'TE: trailers'

stops:
  curl 'https://transit.ttc.com.ge/pis-gateway/api/v2/stops?locale=en' \
  --compressed \
  -H 'User-Agent: Mozilla/5.0 (X11; Linux x86_64; rv:149.0) Gecko/20100101 Firefox/149.0' \
  -H 'Accept: application/json, text/plain, */*' \
  -H 'Accept-Language: en-US,en;q=0.9' \
  -H 'Accept-Encoding: gzip, deflate, br, zstd' \
  -H 'X-api-key: c0a2f304-551a-4d08-b8df-2c53ecd57f9f' \
  -H 'DNT: 1' \
  -H 'Alt-Used: transit.ttc.com.ge' \
  -H 'Connection: keep-alive' \
  -H 'Referer: https://transit.ttc.com.ge/' \
  -H 'Cookie: cookiesession1=678A3E12D36DEB740FA562CFD52AD7AD' \
  -H 'Sec-Fetch-Dest: empty' \
  -H 'Sec-Fetch-Mode: cors' \
  -H 'Sec-Fetch-Site: same-origin' \
  -H 'Sec-GPC: 1' \
  -H 'Pragma: no-cache' \
  -H 'Cache-Control: no-cache'
