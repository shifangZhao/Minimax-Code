---
name: django-patterns
description: Django 架构模式、REST API 设计（DRF）、ORM 最佳实践、缓存、信号、中间件和生产级 Django 应用。
origin: ECC
---

# Django 开发模式

可扩展、可维护应用的生产级 Django 架构模式。

## 激活时机

- 构建 Django Web 应用
- 设计 Django REST Framework API
- 使用 Django ORM 和模型
- 设置 Django 项目结构
- 实现缓存、信号、中间件

## 项目结构

### 推荐布局

```
myproject/
├── config/
│   ├── __init__.py
│   ├── settings/
│   │   ├── __init__.py
│   │   ├── base.py
│   │   ├── local.py
│   │   ├── production.py
│   │   └── test.py
│   ├── urls.py
│   ├── wsgi.py
│   └── asgi.py
├── apps/
│   ├── accounts/
│   │   ├── __init__.py
│   │   ├── models.py
│   │   ├── views.py
│   │   ├── serializers.py
│   │   ├── urls.py
│   │   ├── admin.py
│   │   └── tests/
│   ├── products/
│   │   ├── __init__.py
│   │   ├── models.py
│   │   ├── views.py
│   │   ├── serializers.py
│   │   ├── urls.py
│   │   ├── filters.py
│   │   └── tests/
│   └── orders/
│       └── ...
├── common/
│   ├── __init__.py
│   ├── models.py          # BaseModel, TimestampMixin 等
│   ├── serializers.py     # 通用序列化器
│   ├── permissions.py     # 通用权限
│   └── utils.py
├── scripts/               # 管理脚本
├── tests/                 # 集成测试
├── pyproject.toml
├── manage.py
└── Dockerfile
```

## 模型模式

### 基础模型

```python
# common/models.py
from django.db import models
from django.utils import timezone


class BaseModel(models.Model):
    """所有模型的基础类。"""

    created_at = models.DateTimeField(auto_now_add=True)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        abstract = True


class TimestampMixin(models.Model):
    """时间戳混入。"""

    created_at = models.DateTimeField(default=timezone.now)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        abstract = True
```

### 模型示例

```python
# apps/products/models.py
from django.db import models
from django.contrib.auth import get_user_model
from django.core.validators import MinValueValidator, FileExtensionValidator
from common.models import BaseModel

User = get_user_model()


class Category(BaseModel):
    """产品类别。"""

    name = models.CharField(max_length=100)
    slug = models.SlugField(max_length=100, unique=True)
    description = models.TextField(blank=True)
    parent = models.ForeignKey(
        'self',
        on_delete=models.CASCADE,
        null=True,
        blank=True,
        related_name='children',
    )

    class Meta:
        verbose_name_plural = 'categories'
        ordering = ['name']

    def __str__(self):
        return self.name

    def save(self, *args, **kwargs):
        if not self.slug:
            self.slug = slugify(self.name)
        super().save(*args, **kwargs)


class Product(BaseModel):
    """产品模型。"""

    category = models.ForeignKey(
        Category,
        on_delete=models.SET_NULL,
        null=True,
        related_name='products',
    )
    name = models.CharField(max_length=255)
    slug = models.SlugField(max_length=255, unique=True)
    description = models.TextField(blank=True)
    price = models.DecimalField(
        max_digits=10,
        decimal_places=2,
        validators=[MinValueValidator(0)],
    )
    stock = models.PositiveIntegerField(default=0)
    is_active = models.BooleanField(default=True)
    is_featured = models.BooleanField(default=False)
    image = models.ImageField(
        upload_to='products/%Y/%m/',
        null=True,
        blank=True,
        validators=[FileExtensionValidator(['jpg', 'jpeg', 'png', 'webp'])],
    )
    tags = models.ManyToManyField('Tag', blank=True)
    created_by = models.ForeignKey(
        User,
        on_delete=models.SET_NULL,
        null=True,
        related_name='products',
    )

    class Meta:
        db_table = 'products'
        ordering = ['-created_at']
        indexes = [
            models.Index(fields=['slug']),
            models.Index(fields=['-created_at']),
            models.Index(fields=['category', 'is_active']),
        ]
        constraints = [
            models.CheckConstraint(
                check=models.Q(price__gte=0),
                name='price_non_negative'
            )
        ]

    def __str__(self):
        return self.name

    def save(self, *args, **kwargs):
        if not self.slug:
            self.slug = slugify(self.name)
        super().save(*args, **kwargs)
```

### QuerySet 最佳实践

```python
from django.db import models

class ProductQuerySet(models.QuerySet):
    """Product 模型的自定义 QuerySet。"""

    def active(self):
        """仅返回活跃产品。"""
        return self.filter(is_active=True)

    def with_category(self):
        """选择相关 category 以避免 N+1 查询。"""
        return self.select_related('category')

    def with_tags(self):
        """Prefetch 标签以处理多对多关系。"""
        return self.prefetch_related('tags')

    def in_stock(self):
        """返回库存 > 0 的产品。"""
        return self.filter(stock__gt=0)

    def search(self, query):
        """按名称或描述搜索产品。"""
        return self.filter(
            models.Q(name__icontains=query) |
            models.Q(description__icontains=query)
        )

class Product(models.Model):
    # ... 字段 ...

    objects = ProductQuerySet.as_manager()  # 使用自定义 QuerySet

# 用法
Product.objects.active().with_category().in_stock()
```

### Manager 方法

```python
class ProductManager(models.Manager):
    """复杂查询的自定义 manager。"""

    def get_or_none(self, **kwargs):
        """返回对象或 None 而不是 DoesNotExist。"""
        try:
            return self.get(**kwargs)
        except self.model.DoesNotExist:
            return None

    def create_with_tags(self, name, price, tag_names):
        """创建带有相关标签的产品。"""
        product = self.create(name=name, price=price)
        tags = [Tag.objects.get_or_create(name=name)[0] for name in tag_names]
        product.tags.set(tags)
        return product

    def bulk_update_stock(self, product_ids, quantity):
        """批量更新多个产品的库存。"""
        return self.filter(id__in=product_ids).update(stock=quantity)

# 在模型中
class Product(models.Model):
    # ... 字段 ...
    custom = ProductManager()
```

## Django REST Framework 模式

### 序列化器模式

```python
from rest_framework import serializers
from django.contrib.auth.password_validation import validate_password
from .models import Product, User

class ProductSerializer(serializers.ModelSerializer):
    """Product 模型的序列化器。"""

    category_name = serializers.CharField(source='category.name', read_only=True)
    average_rating = serializers.FloatField(read_only=True)
    discount_price = serializers.SerializerMethodField()

    class Meta:
        model = Product
        fields = [
            'id', 'name', 'slug', 'description', 'price',
            'discount_price', 'stock', 'category_name',
            'average_rating', 'created_at'
        ]
        read_only_fields = ['id', 'slug', 'created_at']

    def get_discount_price(self, obj):
        """如有折扣则计算折扣价格。"""
        if hasattr(obj, 'discount') and obj.discount:
            return obj.price * (1 - obj.discount.percent / 100)
        return obj.price

    def validate_price(self, value):
        """确保价格非负。"""
        if value < 0:
            raise serializers.ValidationError("价格不能为负。")
        return value

class ProductCreateSerializer(serializers.ModelSerializer):
    """用于创建产品的序列化器。"""

    class Meta:
        model = Product
        fields = ['name', 'description', 'price', 'stock', 'category']

    def validate(self, data):
        """多字段自定义验证。"""
        if data['price'] > 10000 and data['stock'] > 100:
            raise serializers.ValidationError(
                "高价值产品不能有大库存。"
            )
        return data

class UserRegistrationSerializer(serializers.ModelSerializer):
    """用户注册的序列化器。"""

    password = serializers.CharField(
        write_only=True,
        required=True,
        validators=[validate_password],
        style={'input_type': 'password'}
    )
    password_confirm = serializers.CharField(write_only=True, style={'input_type': 'password'})

    class Meta:
        model = User
        fields = ['email', 'username', 'password', 'password_confirm']

    def validate(self, data):
        """验证密码匹配。"""
        if data['password'] != data['password_confirm']:
            raise serializers.ValidationError({
                "password_confirm": "密码字段不匹配。"
            })
        return data

    def create(self, validated_data):
        """创建带哈希密码的用户。"""
        validated_data.pop('password_confirm')
        password = validated_data.pop('password')
        user = User.objects.create(**validated_data)
        user.set_password(password)
        user.save()
        return user
```

### ViewSet 模式

```python
from rest_framework import viewsets, status, filters
from rest_framework.decorators import action
from rest_framework.response import Response
from rest_framework.permissions import IsAuthenticated, IsAdminUser
from django_filters.rest_framework import DjangoFilterBackend
from .models import Product
from .serializers import ProductSerializer, ProductCreateSerializer
from .permissions import IsOwnerOrReadOnly
from .filters import ProductFilter
from .services import ProductService

class ProductViewSet(viewsets.ModelViewSet):
    """Product 模型的 ViewSet。"""

    queryset = Product.objects.select_related('category').prefetch_related('tags')
    permission_classes = [IsAuthenticated, IsOwnerOrReadOnly]
    filter_backends = [DjangoFilterBackend, filters.SearchFilter, filters.OrderingFilter]
    filterset_class = ProductFilter
    search_fields = ['name', 'description']
    ordering_fields = ['price', 'created_at', 'name']
    ordering = ['-created_at']

    def get_serializer_class(self):
        """根据操作返回适当的序列化器。"""
        if self.action == 'create':
            return ProductCreateSerializer
        return ProductSerializer

    def perform_create(self, serializer):
        """使用用户上下文保存。"""
        serializer.save(created_by=self.request.user)

    @action(detail=False, methods=['get'])
    def featured(self, request):
        """返回精选产品。"""
        featured = self.queryset.filter(is_featured=True)[:10]
        serializer = self.get_serializer(featured, many=True)
        return Response(serializer.data)

    @action(detail=True, methods=['post'])
    def purchase(self, request, pk=None):
        """购买产品。"""
        product = self.get_object()
        service = ProductService()
        result = service.purchase(product, request.user)
        return Response(result, status=status.HTTP_201_CREATED)

    @action(detail=False, methods=['get'], permission_classes=[IsAuthenticated])
    def my_products(self, request):
        """返回当前用户创建的产品。"""
        products = self.queryset.filter(created_by=request.user)
        page = self.paginate_queryset(products)
        serializer = self.get_serializer(page, many=True)
        return self.get_paginated_response(serializer.data)
```

### 自定义动作

```python
from rest_framework.decorators import api_view, permission_classes
from rest_framework.permissions import IsAuthenticated
from rest_framework.response import Response

@api_view(['POST'])
@permission_classes([IsAuthenticated])
def add_to_cart(request):
    """将产品添加到用户购物车。"""
    product_id = request.data.get('product_id')
    quantity = request.data.get('quantity', 1)

    try:
        product = Product.objects.get(id=product_id)
    except Product.DoesNotExist:
        return Response(
            {'error': '产品未找到'},
            status=status.HTTP_404_NOT_FOUND
        )

    cart, _ = Cart.objects.get_or_create(user=request.user)
    CartItem.objects.create(
        cart=cart,
        product=product,
        quantity=quantity
    )

    return Response({'message': '已添加到购物车'}, status=status.HTTP_201_CREATED)
```

## 服务层模式

```python
# apps/orders/services.py
from typing import Optional
from django.db import transaction
from .models import Order, OrderItem

class OrderService:
    """订单相关业务逻辑的服务层。"""

    @staticmethod
    @transaction.atomic
    def create_order(user, cart: Cart) -> Order:
        """从购物车创建订单。"""
        order = Order.objects.create(
            user=user,
            total_price=cart.total_price
        )

        for item in cart.items.all():
            OrderItem.objects.create(
                order=order,
                product=item.product,
                quantity=item.quantity,
                price=item.product.price
            )

        cart.items.all().delete()
        return order

    @staticmethod
    def cancel_order(order: Order) -> None:
        """取消订单并恢复库存。"""
        with transaction.atomic():
            for item in order.items.all():
                item.product.stock += item.quantity
                item.product.save()
            order.status = 'cancelled'
            order.save()

    @staticmethod
    def get_order_summary(order: Order) -> dict:
        """获取订单摘要。"""
        return {
            'id': order.id,
            'user': order.user.email,
            'total': order.total_price,
            'items_count': order.items.count(),
            'status': order.status,
            'created_at': order.created_at.isoformat(),
        }
```

## 缓存模式

### 缓存配置

```python
# config/settings/production.py
CACHES = {
    'default': {
        'BACKEND': 'django.core.cache.backends.redis.RedisCache',
        'LOCATION': 'redis://127.0.0.1:6379/1',
        'OPTIONS': {
            'CLIENT_CLASS': 'django_redis.client.DefaultClient',
        },
        'KEY_PREFIX': 'myproject',
        'TIMEOUT': 60 * 15,  # 15 分钟默认超时
    }
}
```

### 缓存使用

```python
from django.core.cache import cache

# 简单缓存
def get_product_categories():
    cache_key = 'product_categories'
    data = cache.get(cache_key)
    if data is None:
        data = Category.objects.filter(is_active=True).values_list('id', 'name')
        cache.set(cache_key, data, timeout=60 * 60)  # 1 小时
    return data

# 缓存版本控制
def get_featured_products():
    cache_key = 'featured_products'
    data = cache.get(cache_key)
    if data is None:
        data = list(Product.objects.filter(is_featured=True)[:10].values())
        cache.set(cache_key, data)
    return data

# 清除缓存
def invalidate_featured_products():
    cache.delete('featured_products')
```

### 模板片段缓存

```django
{% load cache %}

{% cache 300 product_list category_id %}
    {% for product in products %}
        <li>{{ product.name }} - {{ product.price }}</li>
    {% endfor %}
{% endcache %}
```

## 信号模式

```python
# apps/products/signals.py
from django.db.models.signals import post_save, post_delete
from django.dispatch import receiver
from .models import Product

@receiver(post_save, sender=Product)
def product_post_save(sender, instance, created, **kwargs):
    """产品保存后清除相关缓存。"""
    from django.core.cache import cache
    cache.delete('featured_products')
    cache.delete(f'product_{instance.id}')

    if created:
        # 发送通知等
        pass

@receiver(post_delete, sender=Product)
def product_post_delete(sender, instance, **kwargs):
    """产品删除后清除缓存。"""
    from django.core.cache import cache
    cache.delete('featured_products')
    cache.delete(f'product_{instance.id}')
```

## 中间件模式

```python
# common/middleware.py
class RequestTimingMiddleware:
    """记录请求处理时间。"""

    def __init__(self, get_response):
        self.get_response = get_response

    def __call__(self, request):
        import time
        start = time.time()
        response = self.get_response(request)
        duration = time.time() - start
        response['X-Request-Duration'] = str(duration)
        return response


class RateLimitMiddleware:
    """简单速率限制中间件。"""

    def __init__(self, get_response):
        self.get_response = get_response
        self.requests = {}

    def __call__(self, request):
        ip = request.META.get('REMOTE_ADDR')
        now = time.time()

        if ip in self.requests:
            timestamps = self.requests[ip]
            timestamps = [t for t in timestamps if now - t < 60]
            self.requests[ip] = timestamps

            if len(timestamps) > 100:  # 每分钟最多 100 请求
                return JsonResponse({'error': '速率限制'}, status=429)
            timestamps.append(now)
        else:
            self.requests[ip] = [now]

        return self.get_response(request)
```

## 分页模式

```python
# 全局分页
REST_FRAMEWORK = {
    'DEFAULT_PAGINATION_CLASS': 'rest_framework.pagination.PageNumberPagination',
    'PAGE_SIZE': 20,
    'DEFAULT_PAGINATION_CLASS': 'apps.common.pagination.CustomPagination',
}

# 自定义分页器
# apps/common/pagination.py
from rest_framework.pagination import PageNumberPagination

class CustomPagination(PageNumberPagination):
    page_size = 20
    page_size_query_param = 'page_size'
    max_page_size = 100

# 在 ViewSet 中使用
class ProductViewSet(viewsets.ModelViewSet):
    pagination_class = CustomPagination
```

## 过滤模式

```python
# apps/products/filters.py
from django_filters import rest_framework as filters
from .models import Product

class ProductFilter(filters.FilterSet):
    category = filters.NumberFilter(field_name='category__id')
    min_price = filters.NumberFilter(field_name='price', lookup_expr='gte')
    max_price = filters.NumberFilter(field_name='price', lookup_expr='lte')
    is_active = filters.BooleanFilter(field_name='is_active')
    search = filters.CharFilter(field_name='name', lookup_expr='icontains')
    created_after = filters.DateFilter(field_name='created_at', lookup_expr='gte')

    class Meta:
        model = Product
        fields = ['category', 'min_price', 'max_price', 'is_active', 'search']
```

## 权限模式

```python
# apps/common/permissions.py
from rest_framework import permissions

class IsOwnerOrReadOnly(permissions.BasePermission):
    """对象级权限：仅所有者可以编辑。"""

    def has_object_permission(self, request, view, obj):
        if request.method in permissions.SAFE_METHODS:
            return True
        return obj.created_by == request.user or request.user.is_staff


class IsAdminOrReadOnly(permissions.BasePermission):
    """仅管理员可以修改，其他仅读。"""

    def has_permission(self, request, view):
        if request.method in permissions.SAFE_METHODS:
            return True
        return request.user and request.user.is_staff
```

## 管理界面模式

```python
# apps/products/admin.py
from django.contrib import admin
from .models import Product, Category

@admin.register(Category)
class CategoryAdmin(admin.ModelAdmin):
    list_display = ['name', 'slug', 'parent', 'created_at']
    prepopulated_fields = {'slug': ('name',)}
    search_fields = ['name']


@admin.register(Product)
class ProductAdmin(admin.ModelAdmin):
    list_display = ['name', 'category', 'price', 'stock', 'is_active', 'is_featured']
    list_filter = ['is_active', 'is_featured', 'category']
    search_fields = ['name', 'description']
    prepopulated_fields = {'slug': ('name',)}
    list_editable = ['price', 'stock', 'is_active', 'is_featured']
    raw_id_fields = ['category', 'created_by']
```

## 测试模式

```python
# apps/products/tests/test_models.py
from django.test import TestCase
from django.contrib.auth import get_user_model
from .models import Category, Product

User = get_user_model()


class ProductModelTests(TestCase):
    """产品模型测试。"""

    def setUp(self):
        self.user = User.objects.create_user(
            username='testuser',
            email='test@example.com',
            password='testpass123'
        )
        self.category = Category.objects.create(
            name='Electronics',
            slug='electronics'
        )

    def test_product_creation(self):
        """测试产品创建。"""
        product = Product.objects.create(
            name='Laptop',
            slug='laptop',
            price=999.99,
            stock=10,
            category=self.category,
            created_by=self.user
        )
        self.assertEqual(product.name, 'Laptop')
        self.assertEqual(product.price, 999.99)
        self.assertTrue(product.is_active)

    def test_product_slug_auto_generation(self):
        """测试产品 slug 自动生成。"""
        product = Product.objects.create(
            name='Smartphone',
            price=599.99,
            category=self.category,
            created_by=self.user
        )
        self.assertEqual(product.slug, 'smartphone')

    def test_product_str(self):
        """测试产品字符串表示。"""
        product = Product.objects.create(
            name='Tablet',
            price=299.99,
            category=self.category,
            created_by=self.user
        )
        self.assertEqual(str(product), 'Tablet')


# apps/products/tests/test_views.py
from rest_framework.test import APITestCase
from rest_framework import status
from django.contrib.auth import get_user_model
from .models import Category, Product

User = get_user_model()


class ProductViewSetTests(APITestCase):
    """产品 ViewSet 测试。"""

    def setUp(self):
        self.user = User.objects.create_user(
            username='testuser',
            email='test@example.com',
            password='testpass123'
        )
        self.category = Category.objects.create(
            name='Electronics',
            slug='electronics'
        )
        self.product = Product.objects.create(
            name='Laptop',
            slug='laptop',
            price=999.99,
            stock=10,
            category=self.category,
            created_by=self.user
        )

    def test_list_products(self):
        """测试列出产品。"""
        response = self.client.get('/api/products/')
        self.assertEqual(response.status_code, status.HTTP_200_OK)

    def test_retrieve_product(self):
        """测试获取单个产品。"""
        response = self.client.get(f'/api/products/{self.product.id}/')
        self.assertEqual(response.status_code, status.HTTP_200_OK)
        self.assertEqual(response.data['name'], 'Laptop')

    def test_create_product_authenticated(self):
        """测试创建产品需要认证。"""
        self.client.force_authenticate(user=self.user)
        data = {
            'name': 'Tablet',
            'price': 299.99,
            'stock': 5,
            'category': self.category.id
        }
        response = self.client.post('/api/products/', data)
        self.assertEqual(response.status_code, status.HTTP_201_CREATED)

    def test_create_product_unauthenticated(self):
        """测试未认证创建产品被拒绝。"""
        data = {
            'name': 'Tablet',
            'price': 299.99,
            'stock': 5,
            'category': self.category.id
        }
        response = self.client.post('/api/products/', data)
        self.assertEqual(response.status_code, status.HTTP_401_UNAUTHORIZED)
```