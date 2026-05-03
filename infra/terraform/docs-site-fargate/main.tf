data "aws_availability_zones" "available" {
  state = "available"
}

resource "aws_vpc" "docs" {
  cidr_block           = local.vpc_cidr
  enable_dns_hostnames = true
  enable_dns_support   = true
  tags = {
    Name = "${local.name_prefix}-docs-vpc"
  }
}

resource "aws_internet_gateway" "docs" {
  vpc_id = aws_vpc.docs.id
  tags = {
    Name = "${local.name_prefix}-docs-igw"
  }
}

resource "aws_subnet" "public" {
  count                   = 2
  vpc_id                  = aws_vpc.docs.id
  cidr_block              = cidrsubnet(local.vpc_cidr, 8, count.index)
  availability_zone       = data.aws_availability_zones.available.names[count.index]
  map_public_ip_on_launch = true
  tags = {
    Name = "${local.name_prefix}-docs-public-${count.index}"
  }
}

resource "aws_route_table" "public" {
  vpc_id = aws_vpc.docs.id
  route {
    cidr_block = "0.0.0.0/0"
    gateway_id = aws_internet_gateway.docs.id
  }
  tags = {
    Name = "${local.name_prefix}-docs-public-rt"
  }
}

resource "aws_route_table_association" "public" {
  count          = 2
  subnet_id      = aws_subnet.public[count.index].id
  route_table_id = aws_route_table.public.id
}

resource "aws_security_group" "alb" {
  name_prefix = "${local.name_prefix}-alb-"
  vpc_id      = aws_vpc.docs.id
  description = "ALB ingress"
  ingress {
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }
  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
  lifecycle {
    create_before_destroy = true
  }
}

resource "aws_security_group" "ecs" {
  name_prefix = "${local.name_prefix}-ecs-"
  vpc_id      = aws_vpc.docs.id
  description = "ECS tasks"
  ingress {
    from_port       = 8080
    to_port         = 8080
    protocol        = "tcp"
    security_groups = [aws_security_group.alb.id]
  }
  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
  lifecycle {
    create_before_destroy = true
  }
}

resource "aws_lb" "docs" {
  name               = substr("${local.name_prefix}-alb", 0, 32)
  internal           = false
  load_balancer_type = "application"
  security_groups    = [aws_security_group.alb.id]
  subnets            = aws_subnet.public[*].id
  tags = {
    Name = "${local.name_prefix}-docs-alb"
  }
}

resource "aws_lb_target_group" "slot_a" {
  name_prefix = "dsa-"
  port        = 8080
  protocol    = "HTTP"
  vpc_id      = aws_vpc.docs.id
  target_type = "ip"
  health_check {
    path                = "/health"
    healthy_threshold   = 2
    unhealthy_threshold = 3
    timeout             = 5
    interval            = 30
    matcher             = "200"
  }
  lifecycle {
    create_before_destroy = true
  }
}

resource "aws_lb_target_group" "slot_b" {
  name_prefix = "dsb-"
  port        = 8080
  protocol    = "HTTP"
  vpc_id      = aws_vpc.docs.id
  target_type = "ip"
  health_check {
    path                = "/health"
    healthy_threshold   = 2
    unhealthy_threshold = 3
    timeout             = 5
    interval            = 30
    matcher             = "200"
  }
  lifecycle {
    create_before_destroy = true
  }
}

resource "aws_lb_listener" "https" {
  load_balancer_arn = aws_lb.docs.arn
  port              = 443
  protocol          = "HTTPS"
  ssl_policy        = "ELBSecurityPolicy-TLS13-1-2-2021-06"
  certificate_arn   = var.certificate_arn
  # Both slots' target groups must be registered on this listener: ECS rejects a
  # service load_balancer block if the TG has no associated load balancer. Route
  # all traffic to slot A until you change weights (ignored below after first apply).
  default_action {
    type = "forward"
    forward {
      target_group {
        arn    = aws_lb_target_group.slot_a.arn
        weight = 100
      }
      target_group {
        arn    = aws_lb_target_group.slot_b.arn
        weight = 0
      }
    }
  }
  lifecycle {
    ignore_changes = [default_action]
  }
}

resource "aws_cloudwatch_log_group" "docs" {
  name              = "/ecs/${local.name_prefix}-docs"
  retention_in_days = 14
}

resource "aws_iam_role" "ecs_execution" {
  name_prefix = "${local.name_prefix}-exec-"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Action = "sts:AssumeRole"
      Effect = "Allow"
      Principal = {
        Service = "ecs-tasks.amazonaws.com"
      }
    }]
  })
}

resource "aws_iam_role_policy_attachment" "ecs_execution" {
  role       = aws_iam_role.ecs_execution.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
}

resource "aws_iam_role_policy" "ecs_execution_secrets" {
  count = var.ghcr_credentials_secret_arn != "" ? 1 : 0
  name  = "${local.name_prefix}-exec-secrets"
  role  = aws_iam_role.ecs_execution.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect = "Allow"
      Action = [
        "secretsmanager:GetSecretValue"
      ]
      Resource = var.ghcr_credentials_secret_arn
    }]
  })
}

resource "aws_iam_role" "ecs_task" {
  name_prefix = "${local.name_prefix}-task-"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Action = "sts:AssumeRole"
      Effect = "Allow"
      Principal = {
        Service = "ecs-tasks.amazonaws.com"
      }
    }]
  })
}

resource "aws_ecs_cluster" "docs" {
  name = local.cluster_name
}

locals {
  container_json = jsonencode([
    merge(
      {
        name      = "docs"
        image     = var.container_image
        essential = true
        portMappings = [{
          containerPort = 8080
          protocol      = "tcp"
        }]
        logConfiguration = {
          logDriver = "awslogs"
          options = {
            "awslogs-group"         = aws_cloudwatch_log_group.docs.name
            "awslogs-region"        = var.aws_region
            "awslogs-stream-prefix" = "docs"
          }
        }
        environment = [
          { name = "ASPNETCORE_URLS", value = "http://0.0.0.0:8080" },
          { name = "M43__StaticAssetsBaseUrl", value = "https://static.michaelj43.dev" }
        ]
      },
      var.ghcr_credentials_secret_arn != "" ? {
        repositoryCredentials = {
          credentialsParameter = var.ghcr_credentials_secret_arn
        }
      } : {}
    )
  ])
}

resource "aws_ecs_task_definition" "slot_a" {
  family                   = "${local.name_prefix}-docs-a"
  network_mode             = "awsvpc"
  requires_compatibilities = ["FARGATE"]
  cpu                      = var.task_cpu
  memory                   = var.task_memory_mb
  execution_role_arn       = aws_iam_role.ecs_execution.arn
  task_role_arn            = aws_iam_role.ecs_task.arn
  container_definitions    = local.container_json
}

resource "aws_ecs_task_definition" "slot_b" {
  family                   = "${local.name_prefix}-docs-b"
  network_mode             = "awsvpc"
  requires_compatibilities = ["FARGATE"]
  cpu                      = var.task_cpu
  memory                   = var.task_memory_mb
  execution_role_arn       = aws_iam_role.ecs_execution.arn
  task_role_arn            = aws_iam_role.ecs_task.arn
  container_definitions    = local.container_json
}

resource "aws_ecs_service" "slot_a" {
  name            = "${local.name_prefix}-docs-a"
  cluster         = aws_ecs_cluster.docs.id
  task_definition = aws_ecs_task_definition.slot_a.arn
  desired_count   = var.desired_count_per_slot
  launch_type     = "FARGATE"
  network_configuration {
    subnets          = aws_subnet.public[*].id
    security_groups  = [aws_security_group.ecs.id]
    assign_public_ip = true
  }
  load_balancer {
    target_group_arn = aws_lb_target_group.slot_a.arn
    container_name   = "docs"
    container_port   = 8080
  }
}

resource "aws_ecs_service" "slot_b" {
  name            = "${local.name_prefix}-docs-b"
  cluster         = aws_ecs_cluster.docs.id
  task_definition = aws_ecs_task_definition.slot_b.arn
  desired_count   = var.desired_count_per_slot
  launch_type     = "FARGATE"
  network_configuration {
    subnets          = aws_subnet.public[*].id
    security_groups  = [aws_security_group.ecs.id]
    assign_public_ip = true
  }
  load_balancer {
    target_group_arn = aws_lb_target_group.slot_b.arn
    container_name   = "docs"
    container_port   = 8080
  }
}

resource "aws_route53_record" "apex" {
  zone_id = var.hosted_zone_id
  name    = var.domain_name
  type    = "A"
  alias {
    name                   = aws_lb.docs.dns_name
    zone_id                = aws_lb.docs.zone_id
    evaluate_target_health = true
  }
}

output "alb_dns_name" {
  value = aws_lb.docs.dns_name
}

output "alb_zone_id" {
  value = aws_lb.docs.zone_id
}

output "listener_arn" {
  value = aws_lb_listener.https.arn
}

output "target_group_a_arn" {
  value = aws_lb_target_group.slot_a.arn
}

output "target_group_b_arn" {
  value = aws_lb_target_group.slot_b.arn
}

output "ecs_cluster_name" {
  value = aws_ecs_cluster.docs.name
}

output "ecs_service_a_name" {
  value = aws_ecs_service.slot_a.name
}

output "ecs_service_b_name" {
  value = aws_ecs_service.slot_b.name
}

output "vpc_id" {
  value = aws_vpc.docs.id
}
