variable "aws_region" {
  type        = string
  description = "Region for S3 bucket (should match primary region)"
  default     = "us-east-1"
}

variable "github_repository" {
  type    = string
  default = "MichaelJ43/local-ai-worker"
}

variable "domain_name" {
  type        = string
  description = "Apex hostname for CloudFront alternate domain"
}

variable "hosted_zone_id" {
  type        = string
  description = "Route53 hosted zone ID for domain_name (delegated zone apex)"
}

variable "cloudfront_certificate_arn" {
  type        = string
  description = "ACM certificate ARN in us-east-1 for CloudFront viewer"
}
