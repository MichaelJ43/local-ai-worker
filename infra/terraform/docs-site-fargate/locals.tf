locals {
  name_prefix = replace(lower(var.github_repository), "/", "-")
  cluster_name = "${local.name_prefix}-docs"
  vpc_cidr     = "10.42.0.0/16"
}
