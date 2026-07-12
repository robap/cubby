// aws-sdk-go-v2 conformance: a >8MB multipart round-trip via the s3 manager
// Uploader, a prefix+delimiter ListObjectsV2, and a credential-less presigned
// GET (s3.PresignClient) against a live cubby (path-style, plain HTTP).
package main

import (
	"context"
	"crypto/md5"
	"encoding/hex"
	"fmt"
	"io"
	"net/http"
	"os"
	"strings"
	"time"

	"github.com/aws/aws-sdk-go-v2/aws"
	"github.com/aws/aws-sdk-go-v2/config"
	"github.com/aws/aws-sdk-go-v2/credentials"
	"github.com/aws/aws-sdk-go-v2/feature/s3/manager"
	"github.com/aws/aws-sdk-go-v2/service/s3"
)

var fails int

func check(name string, cond bool) {
	if cond {
		fmt.Println("ok  : " + name)
	} else {
		fmt.Println("FAIL: " + name)
		fails++
	}
}

func md5File(path string) string {
	f, err := os.Open(path)
	must(err)
	defer f.Close()
	h := md5.New()
	_, err = io.Copy(h, f)
	must(err)
	return hex.EncodeToString(h.Sum(nil))
}

func must(err error) {
	if err != nil {
		panic(err)
	}
}

func main() {
	ctx := context.Background()
	ep := os.Getenv("CUBBY_EP")
	bucket := os.Getenv("CUBBY_BUCKET")
	big := os.Getenv("CUBBY_BIG")

	cfg, err := config.LoadDefaultConfig(ctx,
		config.WithRegion("us-east-1"),
		config.WithCredentialsProvider(
			credentials.NewStaticCredentialsProvider("local", "localsecret", ""),
		),
	)
	must(err)

	client := s3.NewFromConfig(cfg, func(o *s3.Options) {
		o.BaseEndpoint = aws.String(ep)
		o.UsePathStyle = true // cubby is path-style only
	})

	_, err = client.CreateBucket(ctx, &s3.CreateBucketInput{Bucket: aws.String(bucket)})
	must(err)

	// 1) round-trip incl. a >8MB upload via manager.Uploader (5MB parts → a 12MB
	//    body uploads as 3 multipart parts), bytes verified equal.
	f, err := os.Open(big)
	must(err)
	uploader := manager.NewUploader(client, func(u *manager.Uploader) {
		u.PartSize = 5 * 1024 * 1024
	})
	_, err = uploader.Upload(ctx, &s3.PutObjectInput{
		Bucket: aws.String(bucket), Key: aws.String("big.bin"), Body: f,
	})
	must(err)
	f.Close()

	get, err := client.GetObject(ctx, &s3.GetObjectInput{
		Bucket: aws.String(bucket), Key: aws.String("big.bin"),
	})
	must(err)
	h := md5.New()
	_, err = io.Copy(h, get.Body)
	must(err)
	get.Body.Close()
	check("multipart >8MB round-trip: bytes equal", hex.EncodeToString(h.Sum(nil)) == md5File(big))

	head, err := client.HeadObject(ctx, &s3.HeadObjectInput{
		Bucket: aws.String(bucket), Key: aws.String("big.bin"),
	})
	must(err)
	etag := aws.ToString(head.ETag)
	check("multipart ETag is composite (-N): "+etag, strings.Contains(etag, "-"))

	// 2) list a nested layout with prefix + `/` delimiter.
	for _, kv := range [][2]string{
		{"docs/a.txt", "a"}, {"docs/b.txt", "b"}, {"docs/img/c.txt", "c"}, {"top.txt", "t"},
	} {
		_, err = client.PutObject(ctx, &s3.PutObjectInput{
			Bucket: aws.String(bucket), Key: aws.String(kv[0]), Body: strings.NewReader(kv[1]),
		})
		must(err)
	}
	list, err := client.ListObjectsV2(ctx, &s3.ListObjectsV2Input{
		Bucket: aws.String(bucket), Prefix: aws.String("docs/"), Delimiter: aws.String("/"),
	})
	must(err)
	var keys, cps []string
	for _, o := range list.Contents {
		keys = append(keys, aws.ToString(o.Key))
	}
	for _, p := range list.CommonPrefixes {
		cps = append(cps, aws.ToString(p.Prefix))
	}
	check("delimiter list keys == docs/a.txt,docs/b.txt: "+strings.Join(keys, ","),
		strings.Join(keys, ",") == "docs/a.txt,docs/b.txt")
	check("delimiter list CommonPrefixes == docs/img/: "+strings.Join(cps, ","),
		strings.Join(cps, ",") == "docs/img/")

	// 3) presigned GET fetched with no ambient credentials.
	_, err = client.PutObject(ctx, &s3.PutObjectInput{
		Bucket: aws.String(bucket), Key: aws.String("signed.txt"),
		Body: strings.NewReader("presigned-bytes"),
	})
	must(err)
	presign := s3.NewPresignClient(client)
	req, err := presign.PresignGetObject(ctx,
		&s3.GetObjectInput{Bucket: aws.String(bucket), Key: aws.String("signed.txt")},
		func(o *s3.PresignOptions) { o.Expires = 5 * time.Minute },
	)
	must(err)
	resp, err := http.Get(req.URL) // no credentials
	must(err)
	body, _ := io.ReadAll(resp.Body)
	resp.Body.Close()
	check(fmt.Sprintf("presigned GET returns 200 + bytes (status %d)", resp.StatusCode),
		resp.StatusCode == 200 && string(body) == "presigned-bytes")

	if fails > 0 {
		os.Exit(1)
	}
}
