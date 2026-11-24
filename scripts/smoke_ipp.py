"""Exercise real HTTP/IPP and PWG decoding. Refuses to print to a non-simulated bridge."""
import argparse
import http.client
import struct
import time


def attr(tag, name, value):
    name = name.encode()
    if isinstance(value, str):
        value = value.encode()
    return bytes([tag]) + struct.pack(">H", len(name)) + name + struct.pack(">H", len(value)) + value


def integer(name, value):
    return attr(0x21, name, struct.pack(">i", value))


def parse(data):
    status, request = struct.unpack_from(">Hi", data, 2)
    values = {}
    cursor = 8
    name = ""
    while data[cursor] != 3:
        tag = data[cursor]
        cursor += 1
        if tag < 16:
            continue
        size = struct.unpack_from(">H", data, cursor)[0]
        cursor += 2
        if size:
            name = data[cursor:cursor+size].decode()
        cursor += size
        size = struct.unpack_from(">H", data, cursor)[0]
        cursor += 2
        value = data[cursor:cursor+size]
        cursor += size
        if tag in (0x21, 0x23) and size == 4:
            value = struct.unpack(">i", value)[0]
        values.setdefault(name, value)
    return status, values


def page(width=112, height=320, pattern=0):
    header = bytearray(1796)
    header[:9] = b"PwgRaster"
    fields = {276:203, 280:203, 340:1, 372:width, 376:height,
              384:1, 388:1, 392:(width+7)//8, 396:0, 400:3, 420:1}
    for offset, value in fields.items():
        struct.pack_into(">I", header, offset, value)
    out = header
    for y in range(height):
        row = bytearray((width+7)//8)
        for x in range(width):
            black = x < 2 or x >= width-2 or y < 2 or y >= height-2 or (20 <= y < 40+pattern*10 and 15 <= x < width-15)
            if black:
                row[x//8] |= 0x80 >> (x % 8)
        out.append(0)  # one row
        for i in range(0, len(row), 128):
            chunk = row[i:i+128]
            if len(chunk) == 1:
                out.extend([0, chunk[0]])
            else:
                out.append(257-len(chunk))
                out.extend(chunk)
    return bytes(out)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=18631)
    args = parser.parse_args()
    request_id = 0

    def call(operation, attributes=b"", document=b"", chunked=False):
        nonlocal request_id
        request_id += 1
        data = struct.pack(">BBHi", 2, 0, operation, request_id) + b"\x01"
        data += attr(0x47, "attributes-charset", "utf-8")
        data += attr(0x48, "attributes-natural-language", "en")
        data += attr(0x45, "printer-uri", f"ipp://127.0.0.1:{args.port}/ipp/print")
        data += attributes + b"\x03" + document
        client = http.client.HTTPConnection("127.0.0.1", args.port, timeout=10)
        body = (data[i:i+137] for i in range(0, len(data), 137)) if chunked else data
        client.request("POST", "/ipp/print", body=body,
                       headers={"Content-Type":"application/ipp"}, encode_chunked=chunked)
        response = client.getresponse()
        assert response.status == 200, response.status
        result = parse(response.read())
        client.close()
        return result

    status, capabilities = call(11)
    assert status == 0
    assert b"SIMULATION" in capabilities.get("printer-info", b""), "REFUSED: bridge is not in --simulate mode"
    assert capabilities["printer-name"] == b"Niimbot D101"
    print("PASS capabilities and simulation guard")

    status, job = call(2, attr(0x49,"document-format","image/pwg-raster") +
                       attr(0x42,"job-name","HTTP smoke: two pages x two copies") + b"\x02" + integer("copies",2),
                       b"RaS2"+page(pattern=0)+page(pattern=1), chunked=True)
    assert status == 0, (status, job)
    job_id = job["job-id"]
    for _ in range(100):
        status, job = call(9, integer("job-id",job_id))
        if job["job-state"] >= 7:
            break
        time.sleep(0.1)
    assert job["job-state"] == 9, job
    assert job["job-impressions-completed"] == 4, job
    print("PASS chunked HTTP, multi-page PWG, four confirmed simulated labels")

    status, held = call(5, attr(0x42,"job-name","Create-Job smoke"))
    assert status == 0 and held["job-state"] == 4
    attributes = integer("job-id",held["job-id"])+attr(0x22,"last-document",b"\x01")+attr(0x49,"document-format","image/pwg-raster")
    status, _ = call(6, attributes, b"RaS2"+page())
    assert status == 0
    status, _ = call(6, attributes, b"RaS2"+page())
    assert status == 0x0404
    print("PASS Create-Job / Send-Document and duplicate rejection")

    status, held = call(5)
    assert status == 0
    assert call(8,integer("job-id",held["job-id"]))[0] == 0
    assert call(9,integer("job-id",held["job-id"]))[1]["job-state"] == 7
    assert call(4,attr(0x49,"document-format","application/pdf"))[0] == 0x040a
    assert call(2,attr(0x49,"document-format","image/pwg-raster"),b"RaS2"+page(width=300))[0] == 0x0411
    print("PASS cancellation, unsupported PDF and oversized media rejection")


if __name__ == "__main__":
    main()
