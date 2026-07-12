// aws-sdk-js v3 conformance: a >8MB multipart round-trip via @aws-sdk/lib-storage
// `Upload`, a prefix+delimiter listing, and a credential-less presigned GET.

import {
  S3Client,
  CreateBucketCommand,
  PutObjectCommand,
  GetObjectCommand,
  HeadObjectCommand,
  ListObjectsV2Command,
} from "@aws-sdk/client-s3";
import { Upload } from "@aws-sdk/lib-storage";
import { getSignedUrl } from "@aws-sdk/s3-request-presigner";
import { createReadStream } from "node:fs";
import { createHash } from "node:crypto";

const EP = process.env.CUBBY_EP;
const BUCKET = process.env.CUBBY_BUCKET;
const BIG = process.env.CUBBY_BIG;

const s3 = new S3Client({
  endpoint: EP,
  region: "us-east-1",
  credentials: { accessKeyId: "local", secretAccessKey: "localsecret" },
  forcePathStyle: true, // cubby is path-style only
});

let fails = 0;
function check(name, cond) {
  console.log((cond ? "ok  : " : "FAIL: ") + name);
  if (!cond) fails++;
}

function md5File(path) {
  return new Promise((resolve, reject) => {
    const h = createHash("md5");
    createReadStream(path)
      .on("data", (d) => h.update(d))
      .on("end", () => resolve(h.digest("hex")))
      .on("error", reject);
  });
}

async function md5Stream(body) {
  const h = createHash("md5");
  for await (const chunk of body) h.update(chunk);
  return h.digest("hex");
}

await s3.send(new CreateBucketCommand({ Bucket: BUCKET }));

// 1) round-trip incl. a >8MB upload driven by lib-storage `Upload` (5MB parts →
//    a 12MB body uploads as 3 multipart parts), bytes verified equal.
const up = new Upload({
  client: s3,
  params: { Bucket: BUCKET, Key: "big.bin", Body: createReadStream(BIG) },
  queueSize: 4,
  partSize: 5 * 1024 * 1024,
});
await up.done();
const got = await s3.send(new GetObjectCommand({ Bucket: BUCKET, Key: "big.bin" }));
const dl = await md5Stream(got.Body);
check("multipart >8MB round-trip: bytes equal", dl === (await md5File(BIG)));
const head = await s3.send(new HeadObjectCommand({ Bucket: BUCKET, Key: "big.bin" }));
check(`multipart ETag is composite (-N): ${head.ETag}`, head.ETag.includes("-"));

// 2) list a nested layout with prefix + `/` delimiter.
for (const [Key, Body] of [
  ["docs/a.txt", "a"],
  ["docs/b.txt", "b"],
  ["docs/img/c.txt", "c"],
  ["top.txt", "t"],
]) {
  await s3.send(new PutObjectCommand({ Bucket: BUCKET, Key, Body }));
}
const list = await s3.send(
  new ListObjectsV2Command({ Bucket: BUCKET, Prefix: "docs/", Delimiter: "/" }),
);
const keys = (list.Contents || []).map((o) => o.Key);
const cps = (list.CommonPrefixes || []).map((p) => p.Prefix);
check(
  `delimiter list keys == [docs/a.txt, docs/b.txt]: ${keys}`,
  JSON.stringify(keys) === JSON.stringify(["docs/a.txt", "docs/b.txt"]),
);
check(
  `delimiter list CommonPrefixes == [docs/img/]: ${cps}`,
  JSON.stringify(cps) === JSON.stringify(["docs/img/"]),
);

// 3) presigned GET fetched with no ambient credentials.
await s3.send(
  new PutObjectCommand({ Bucket: BUCKET, Key: "signed.txt", Body: "presigned-bytes" }),
);
const url = await getSignedUrl(
  s3,
  new GetObjectCommand({ Bucket: BUCKET, Key: "signed.txt" }),
  { expiresIn: 300 },
);
const resp = await fetch(url); // no credentials
const text = await resp.text();
check(
  `presigned GET returns 200 + bytes (status ${resp.status})`,
  resp.status === 200 && text === "presigned-bytes",
);

process.exit(fails ? 1 : 0);
