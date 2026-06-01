# Terraform — docs site

Two separate **S3 backend state keys** (same bucket + lock table you configure in GitHub secrets):

| Path | State key | Purpose |
|------|-----------|---------|
| [`docs-site-fargate/`](docs-site-fargate/) | `docs-site/fargate/terraform.tfstate` | VPC, ALB, ECS (dual service / blue-green slots), Route53 apex → ALB |
| [`docs-site-static/`](docs-site-static/) | `docs-site/static/terraform.tfstate` | S3 bucket, CloudFront (OAC), bucket policy |

## First-time init (local)

```bash
cd infra/terraform/docs-site-fargate
terraform init \
  -backend-config="bucket=YOUR_STATE_BUCKET" \
  -backend-config="key=docs-site/fargate/terraform.tfstate" \
  -backend-config="region=us-east-1" \
  -backend-config="dynamodb_table=YOUR_LOCK_TABLE"
```

Pass variables with `-var` / `TF_VAR_*` or a `terraform.tfvars` file (do not commit secrets).

## CloudFront certificate

`docs-site-static` requires **`TF_VAR_cloudfront_certificate_arn`** for an ACM certificate in **`us-east-1`** (CloudFront requirement).

Soft/full destroy workflows resolve the ARN in this order:

1. **`TF_CLOUDFRONT_ACM_CERTIFICATE_ARN`** — optional override (us-east-1 ACM)
2. **`TF_ACM_CERTIFICATE_ARN`** — reused when **`AWS_REGION`** is **`us-east-1`** (same ARN as the ALB)

If the ALB stack runs outside **`us-east-1`**, request a separate us-east-1 cert for the same domain and set **`TF_CLOUDFRONT_ACM_CERTIFICATE_ARN`**.

## Soft destroy / DNS

After static content is live on CloudFront, soft destroy **automates** Route53 apex → CloudFront (via `docs-site-static` Terraform) and Fargate teardown. Before `terraform destroy` on fargate, the workflow removes `aws_route53_record.apex` from **fargate** state so destroy does not delete the apex alias (see [`.github/workflows/docs-site-soft-destroy.yml`](../../.github/workflows/docs-site-soft-destroy.yml)).

Manual equivalent if you need to run locally:

```bash
cd infra/terraform/docs-site-fargate
terraform state show aws_route53_record.apex && terraform state rm aws_route53_record.apex
terraform destroy
```
