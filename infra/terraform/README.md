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

`docs-site-static` requires **`TF_VAR_cloudfront_certificate_arn`** for an ACM certificate in **`us-east-1`** (CloudFront requirement). Store it as GitHub secret **`TF_CLOUDFRONT_ACM_CERTIFICATE_ARN`** (can match your wildcard apex cert if issued in us-east-1).

## Soft destroy / DNS

After static content is live on CloudFront, update the Route53 apex alias from ALB to CloudFront. If Terraform managed the apex record in the fargate stack, remove it from state before destroying fargate so DNS is not deleted unintentionally:

```bash
cd infra/terraform/docs-site-fargate
terraform state rm aws_route53_record.apex
terraform destroy
```

Automating this cutover is tracked in [`.github/workflows/docs-site-soft-destroy.yml`](../../.github/workflows/docs-site-soft-destroy.yml) (extend with `aws route53 change-resource-record-sets`).
