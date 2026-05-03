variable "aws_region" {
  type        = string
  description = "AWS region for ALB, ECS, VPC"
}

variable "domain_name" {
  type        = string
  description = "Apex hostname e.g. aiworkers.michaelj43.dev"
}

variable "hosted_zone_id" {
  type        = string
  description = "Route53 hosted zone ID"
}

variable "certificate_arn" {
  type        = string
  description = "ACM certificate ARN in var.aws_region (ALB)"
}

variable "container_image" {
  type        = string
  description = "Full image URI e.g. ghcr.io/org/local-ai-worker-docs:tag"
}

variable "github_repository" {
  type        = string
  default     = "MichaelJ43/local-ai-worker"
  description = "Used for naming and tags"
}

variable "ghcr_credentials_secret_arn" {
  type        = string
  default     = ""
  description = "Secrets Manager secret ARN for GHCR docker auth (optional if image is public)"
}

variable "task_cpu" {
  type    = number
  default = 256
}

variable "task_memory_mb" {
  type    = number
  default = 512
}

variable "desired_count_per_slot" {
  type    = number
  default = 1
}
