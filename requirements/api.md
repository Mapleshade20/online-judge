# OJ 系统 API 定义

OJ 系统需要按照下面的要求实现一系列的 API。

注意功能和 API 的对应关系，以及 API 中一些字段只有在实现对应的功能后才需要处理。

通常来说，API 会分为如下的两种：

1. `GET`：HTTP GET 请求，参数通过 URL 传递，一般是用来获取数据；
2. `POST/PUT/DELETE`：HTTP POST/PUT/DELETE 请求，参数以 JSON 格式通过请求正文，一般是进行新建/更新/删除操作。

OJ 系统在处理 API 请求后，会发送响应，并设置状态码：

1. 状态码为 `HTTP 200 OK` 表示请求成功，响应正文见各 API 定义；
2. 状态码 >= 400 表示请求内容有误，>= 500 表示 OJ 在处理请求过程中出现错误。在正文中以 JSON 格式给出形如下面的错误信息：

```json
{
  "code": 3,
  "reason": "ERR_NOT_FOUND",
  "message": "Problem xxx not found."
}
```

具体可能出现哪些错误，见各 API 定义。通常来说，有如下几种错误：

1. `reason=ERR_INVALID_ARGUMENT, code=1, HTTP 400 Bad Request`：不适合其他错误的参数问题
2. `reason=ERR_INVALID_STATE, code=2, HTTP 400 Bad Request`：对象在目前状态下无法进行此操作
3. `reason=ERR_NOT_FOUND, code=3, HTTP 404 Not Found`：无法找到对应的对象
4. `reason=ERR_RATE_LIMIT, code=4, HTTP 400 Bad Request`：超出提交次数限制
5. `reason=ERR_EXTERNAL, code=5, HTTP 500 Internal Server Error`：外部异常，如无法连接到数据库
6. `reason=ERR_INTERNAL, code=6, HTTP 500 Internal Server Error`：内部异常，用于其他错误原因没有覆盖到的问题

## 评测任务相关

### POST /jobs

提交代码以创建一个新的评测任务。

=== "请求"

    请求应附带 JSON 格式的正文。样例：

    ```json
    {
      "source_code": "fn main() { println!(\"Hello, world!\"); }",
      "language": "Rust",
      "user_id": 0,
      "contest_id": 0,
      "problem_id": 0
    }
    ```

    正文的 JSON 应该包含一个字典，字典的各字段含义如下：

    1. `source_code`：必选，提交的源代码
    2. `language`：必选，编程语言
    3. `user_id`：必选，用户 ID，如果未实现用户管理功能，则恒为 0
    4. `contest_id`：必选，比赛 ID，如果未实现比赛功能，则恒为 0
    5. `problem_id`：必选，题目 ID

=== "行为"

    OJ 接受到请求后，首先应当检查请求的合法性，包括：

    1. 检查编程语言是否在配置中
    2. 检查题目 ID 是否在配置中
    3. 如果实现了用户管理功能，检查用户 ID 是否存在
    4. 如果实现了比赛功能，检查比赛 ID 是否存在，检查用户 ID 是否在此比赛中，检查题目 ID 是否在此比赛中，用户该题目的提交次数限制是否达到上限，提交评测任务时间是否在比赛进行时间范围内

    如果请求不合法，则设置错误响应，响应内容见后。

    如果请求合法，则进行评测。如果实现了阻塞评测，则在评测结束时发送响应；如果实现了非阻塞评测，则立即发送响应。

    如果已有评测任务，则新评测任务 ID 为现有评测任务 ID 最大值加一，否则为 0。

=== "响应"

    如果任务被成功创建，则：

    HTTP 状态码为 200 OK，并附带 JSON 格式的正文。样例：

    ```json
    {
      "id": 0,
      "created_time": "2022-08-27T02:05:29.000Z",
      "updated_time": "2022-08-27T02:05:30.000Z",
      "submission": {
        "source_code": "fn main() { println!('Hello World!'); }",
        "language": "Rust",
        "user_id": 0,
        "contest_id": 0,
        "problem_id": 0
      },
      "state": "Queueing",
      "result": "Waiting",
      "score": 87.5,
      "cases": [
        {
          "id": 0,
          "result": "Waiting",
          "time": 0,
          "memory": 0,
          "info": ""
        },
        {
          "id": 1,
          "result": "Waiting",
          "time": 0,
          "memory": 0,
          "info": ""
        }
      ]
    }
    ```

    正文的 JSON 应该包含一个字典，字典的各字段含义如下：

    1. `id`: 必选，新建的评测任务的 ID
    2. `created_time`：必选，提交时间，是评测任务新建的时间（时区为 UTC+0），格式为 `%Y-%m-%dT%H:%M:%S%.3fZ`
    3. `updated_time`：必选，是评测任务最后一次更新的时间（时区为 UTC+0），格式为 `%Y-%m-%dT%H:%M:%S%.3fZ`
    4. `submission`：必选，与请求的正文内容相同
    5. `state`：必选，评测任务的状态，可能的取值有：`Queueing`（正在排队等待评测），`Running`（正在评测），`Finished`（已完成评测），`Canceled`（被取消评测）
    6. `result`：必选，评测任务的结果，可能的取值有：`Waiting`（等待评测），`Running`（正在运行），`Accepted`（通过），`Compilation Error`（编译错误），`Compilation Success`（编译成功），`Wrong Answer`（答案错误），`Runtime Error`（运行时错误，程序异常退出），`Time Limit Exceeded`（超出时间限制），`Memory Limit Exceeded`（超出内存限制），`System Error`（OJ 评测时出现故障），`SPJ Error`（Special Judge 出错），`Skipped`（跳过）
    7. `score`：必选，分数
    8. `cases`：必选，是一个 JSON 数组，每一项的字段含义：
        1. `id`：必选，从 1 开始的数据点编号，0 表示编译
        2. `result`：必选，数据点或编译的结果，可能的取值与上面 `result` 一样
        3. `time`：必选，数据点评测或编译的运行的真实时间（整数，单位是 us），如果还没运行，则为 0
        4. `memory`：必选，数据点评测或编译和内存占用（工作集大小，单位是字节），如果还没运行，或者没有实现内存占用的测量功能，则为 0
        5. `info`：必选，数据点评测或编译的附加信息，如果没有则为空字符串


    注意区分评测任务的创建时间（`created_time`，也是用户的提交时间）和更新时间（`updated_time`）。当一个任务创建了以后，它的创建时间就不再变化了。而每当任务状态更新，无论是评测出了新的结果，还是重新评测，都需要设置更新时间。

    评测任务包括了三种 `result` 字段：

    1. 评测任务的结果，下记为 `job_result`
    2. 编译的结果（`id` 为 0），下记为 `compilation_result`
    3. 数据点的结果（`id` 不为 0），下记为 `case_result`

    评测在以下几个步骤中状态和结果的变化：

    1. 等待评测：此时 `state` 为 `Queueing`，`job_result`，`compilation_result` 和 `case_result` 都为 `Waiting`
    2. 开始编译：此时 `state` 为 `Running`，`job_result` 变为 `Running`，`compilation_result` 变为 `Running`
    3. 编译完成：如果编译成功，则 `compilation_result` 变为 `Compilation Success`，继续进行数据点的评测；如果编译失败，则 `compilation_result` 变为 `Compilation Error`，`job_result` 变为 `Compilation Error`，`state` 变为 `Finished`，评测结束
    4. 开始数据点评测：逐个评测数据点，此时 `case_result` 根据实际情况可能为 `Waiting`，`Running`，`Accepted`，`Wrong Answer`，`Runtime Error`，`Time Limit Exceeded`，`Memory Limit Exceeded`，`System Error`，`SPJ Error`，`Skipped`；只要有其中一个数据点出现了错误（处于除了 `Waiting`，`Running`，`Accepted`，`Skipped` 以外的状态），那么 `job_result` 就变为第一个出现错误的点的状态
    5. 完成数据点评测：所有数据点评测完成后，`state` 变为 `Finished`，如果所有数据点评测结果都是 `Accepted`，则 `job_result` 变为 `Accepted`

    状态 `state` 的状态转移：

    ```mermaid
    flowchart LR
      Queueing -- 开始编译 --> Running -- 评测完成 --> Finished;
      Finished -- 重新评测 --> Queueing;
      Queueing -- 取消评测 --> Canceled;
    ```

    任务结果 `job_result` 的状态转移：

    ```mermaid
    flowchart LR
      Waiting -- 开始编译 --> Running;
      Running -- 编译失败 --> CE[Compilation Error];
      Running -- 数据点出现错误 --> 第一个出错数据点的状态;
      Running -- 所有数据点正确 --> Accepted;
    ```

    编译结果 `compilation_result` 的状态转移：

    ```mermaid
    flowchart LR
      Waiting -- 开始编译 --> Running;
      Running -- 编译失败 --> CE[Compilation Error];
      Running -- 编译成功 --> CS[Compilation Success];
    ```

=== "错误"

    下面是可能出现的错误原因：

    1. `reason=ERR_INVALID_ARGUMENT, code=1, HTTP 400 Bad Request`：用户不在比赛中，或题目不在比赛中，或比赛尚未开始，或比赛已经结束
    2. `reason=ERR_NOT_FOUND, code=3, HTTP 404 Not Found`：编程语言或题目 ID 或用户 ID 或比赛 ID 不存在
    3. `reason=ERR_RATE_LIMIT, code=4, HTTP 400 Bad Request`：超出提交次数限制
    4. `reason=ERR_EXTERNAL, code=5, HTTP 500 Internal Server Error`：外部异常，如无法连接到数据库
    5. `reason=ERR_INTERNAL, code=6, HTTP 500 Internal Server Error`：内部异常，用于其他错误原因没有覆盖到的问题

### GET /jobs

根据 URL 参数查询和筛选评测任务。返回的结果按照任务创建时间升序排序。

=== "请求"

    请求应在 URL 上附带参数，如：

    ```text
    GET http://localhost:12345/jobs?problem_id=0&state=Finished
    ```

    所有可能出现的参数有：

    1. `user_id`：可选，按照用户 ID 进行筛选，未实现用户管理功能可忽略
    2. `user_name`：可选，按照用户名进行筛选，未实现用户管理功能可忽略
    3. `contest_id`：可选，按照比赛 ID 进行筛选，未实现比赛功能可忽略
    4. `problem_id`：可选，按照题目 ID 进行筛选
    5. `language`：可选，按照编程语言进行筛选
    6. `from`：可选，筛选出创建时间不早于该参数的评测任务，时区为 UTC+0，格式为 `%Y-%m-%dT%H:%M:%S%.3fZ`
    7. `to`：可选，筛选出创建时间不晚于该参数的评测任务，时区为 UTC+0，格式为 `%Y-%m-%dT%H:%M:%S%.3fZ`
    8. `state`：可选，按照评测任务当前状态筛选
    9. `result`：可选，按照评测任务当前结果筛选

    每种参数最多出现一次，即不会对同一个字段进行多次筛选。需要对筛选值进行格式检查，例如 `user_id` 需要是整数，`user_name` 需要是字符串，`from` 需要是合法的日期等等。但如果出现了不存在的筛选值（例如按照 user_id=1234 筛选，但是不存在这个用户），或者 `from` 在 `to` 的未来，正常进行过滤，因为没有匹配的项目，所以返回一个空数组。这样设计是为了避免用户探测其他用户是否存在。

=== "行为"

    根据请求中的参数进行筛选，找到满足所有出现的条件的评测任务列表，将实时状态按照创建时间升序作为响应返回。

    如果实现了阻塞评测，那么尚未结束的评测可以出现或不出现在响应当中；如果实现了非阻塞评测，那么尚未结束的评测应当出现在响应当中。

=== "响应"

    如果请求中的参数没有出现格式问题（例如 `user_id=abcd` 或者 `state=ABCDEFG`），则以 JSON 数组的形式返回结果，如：

    ```json
    [
      {
        "id": 0,
        "created_time": "2022-08-27T02:05:29.000Z",
        "updated_time": "2022-08-27T02:05:30.000Z",
        "submission": {
          "source_code": "fn main() { println!('Hello World!'); }",
          "language": "Rust",
          "user_id": 0,
          "contest_id": 0,
          "problem_id": 0
        },
        "state": "Queueing",
        "result": "Waiting",
        "score": 87.5,
        "cases": [
          {
            "id": 0,
            "result": "Waiting",
            "time": 0,
            "memory": 0,
            "info": ""
          },
          {
            "id": 1,
            "result": "Waiting",
            "time": 0,
            "memory": 0,
            "info": ""
          }
        ]
      }
    ]
    ```

    数组的每一项都是一个评测任务，与前述 `POST /jobs` 的响应格式一样。

=== "错误"

    * 请求格式出现错误：HTTP 400，`reason=ERR_INVALID_ARGUMENT, code=1, message="Invalid argument xxx` 或框架自动检测并生成的错误

### GET /jobs/{jobId}

获取单个评测任务信息。

=== "请求"

    请求应在 URL 路径上传递评测任务 id，如：

    ```text
    GET http://localhost:12345/jobs/1
    ```

    表示查询 ID 为 1 的评测任务。

=== "行为"

    根据 URL 中的评测任务 id 找到评测任务并发送响应。

    如果实现了阻塞评测，且该评测任务尚未结束，可以返回评测任务的当前状态，也可以按找不到评测任务处理。

=== "响应"

    如果找到了评测任务，则以 JSON 的形式返回结果，其内容与 `POST /jobs` 的响应一致。

=== "错误"

    * 找不到评测任务：HTTP 404 Not Found，`reason=ERR_NOT_FOUND, code=3, message="Job xxx not found."`

### PUT /jobs/{jobId}

重新评测单个评测任务。

在真实 OJ 系统中，重新评测功能是给出题人使用的，例如比赛中途发现数据有误，可以在修改题目数据后，对已有的评测任务进行重新评测。

=== "请求"

    请求应在 URL 路径上传递评测任务 id，如：

    ```text
    PUT http://localhost:12345/jobs/1
    ```

    表示重新评测 ID 为 1 的评测任务。

=== "行为"

    根据 URL 中的评测任务 id 找到评测任务。如果评测任务处于 Finished 状态，则重新进行评测，如果实现了阻塞评测，则在评测结束时发送响应，如果实现了非阻塞评测，则立即发送响应；否则返回错误。

    重新评测时，直接修改已有评测任务的状态。但其提交内容和提交时间不会改变。

    如果实现了用户多角色支持，还需要判断用户是否有权限进行重新评测，例如普通用户不能重新评测。

=== "响应"

    如果任务成功重新评测，则设置 HTTP 状态码为 HTTP 200 OK，JSON 格式的正文为评测任务，内容与 `POST /jobs` 的响应一致。

=== "错误"

    * 找不到评测任务：HTTP 404 Not Found，`reason=ERR_NOT_FOUND, code=3, message="Job xxx not found."`
    * 评测任务不处在 `Finished` 状态：HTTP 400 Bad Request，`reason=ERR_INVALID_STATE, code=2, message="Job xxx not finished."`

### DELETE /jobs/{jobId}

取消正在等待评测的单个评测任务。

=== "请求"

    请求应在 URL 路径上传递评测任务 id，如：

    ```text
    DELETE http://localhost:12345/jobs/1
    ```

    表示取消评测 ID 为 1 的评测任务。

=== "行为"

    根据 URL 中的评测任务 id 找到评测任务。如果评测任务处于 Queueing 状态，则从评测队列中删除，设置状态码为 HTTP 200 OK，正文不附带内容；如果评测任务处于其他状态，则返回错误响应。

=== "响应"

    如果任务是 Queueing 状态，从评测队列中删除后，设置 HTTP 状态码为 HTTP 200 OK，响应为空。

=== "错误"

     * 没有找到评测任务：HTTP 404 Not Found，`reason=ERR_NOT_FOUND, code=3, message="Job xxx not found."`
     * 评测任务不处在 `Queueing`：HTTP 400 Bad Request，`reason=ERR_INVALID_STATE, code=2, message="Job xxx not queueing."`

## 用户相关

### POST /users

创建新用户或更新已有用户。

=== "请求"

    请求应附带 JSON 格式的正文。样例：

    设置 ID 为 0 的用户名为 `root`：

    ```json
    {
      "id": 0,
      "name": "root"
    }
    ```

    新建用户，用户名为 `user`：

    ```json
    {
      "name": "user"
    }
    ```

    正文的 JSON 应该包含一个字典，字典的各字段含义如下：

    1. `id`：可选，如果提供了 ID，则更新 ID 对应的用户的用户名；如果没有提供，则新建一个用户
    2. `name`：必选，用户名

=== "行为"

    OJ 接受到请求后，如果 `id` 字段存在，则要找到对应的用户，判断新用户名是否与其他用户重名，如果不重名则更新其用户名。如果用户 ID 不存在或出现重名，返回错误响应；如果用户 ID 存在，更新用户名并返回用户信息响应。

    如果 `id` 字段不存在，则查找是否已有用户与要新建的用户重名。如果出现重名，返回错误响应；如果没有出现重名，则新建用户并返回用户信息响应。新建的用户保证其 `id` 和 `name` 都不与现有用户重复。

    新建用户时，如果已有用户，则新用户 ID 为现有用户 ID 最大值加一，否则为 0。

=== "响应"

    如果用户被成功更新或者创建，则：

    HTTP 状态码为 200 OK，并附带 JSON 格式的正文。样例：

    ```json
    {
      "id": 0,
      "name": "root"
    }
    ```

    正文的 JSON 应该包含一个字典，字典的各字段含义如下：

    1. `id`: 必选，用户 ID
    2. `name`：必选，用户名

=== "错误"

     * 根据 ID 找不到用户：HTTP 404 Not Found，`reason=ERR_NOT_FOUND, code=3, message="User xxx not found."`
     * 出现重名：HTTP 400 Bad Request，`reason=ERR_INVALID_ARGUMENT, code=1, message="User name 'xxx' already exists."`

### GET /users

获取用户列表。

=== "请求"

    请求不需要附带参数。

=== "行为"

    以 JSON 响应返回所有用户，按照 ID 升序排列。

=== "响应"

    HTTP 状态码为 200 OK，并附带 JSON 格式的正文。样例：

    ```json
    [
      {
        "id": 0,
        "name": "root"
      },
      {
        "id": 1,
        "name": "user"
      }
    ]
    ```

    正文的 JSON 应该包含一个数组，数组的每一项是一个字典，每个字典对应一个用户，字典的各字段含义如下：

    1. `id`: 必选，用户 ID
    2. `name`：必选，用户名

## 比赛相关

### POST /contests

创建新比赛或更新比赛内容。

=== "请求"

    请求应附带 JSON 格式的正文。样例：

    ```json
    {
      "id": 1,
      "name": "Rust Course Project 2",
      "from": "2022-08-27T02:05:29.000Z",
      "to": "2022-08-27T02:05:30.000Z",
      "problem_ids": [
        2,
        1,
        3
      ],
      "user_ids": [
        5,
        4,
        6
      ],
      "submission_limit": 32
    }
    ```

    正文的 JSON 应该包含一个字典，字典的各字段含义如下：

    1. `id`：可选，如果指定了 ID，则要更新比赛；如果没有指定 ID，则要创建新比赛
    2. `name`：必选，比赛名称
    3. `from`：必选，比赛开始时间，时区为 UTC，格式为 `%Y-%m-%dT%H:%M:%S%.3fZ`
    4. `to`：必选，比赛结束时间，时区为 UTC，格式为 `%Y-%m-%dT%H:%M:%S%.3fZ`
    5. `problem_ids`：必选，一个数组，比赛中所有题目的 ID，不允许出现重复
    6. `user_ids`：必选，一个数组，比赛中所有用户的 ID，不允许出现重复
    7. `submission_limit`：必选，提交次数限制，即每个用户在每个题目上提交次数的最大值，如果不限制，则为 0


=== "行为"

    OJ 接受到请求后，如果 `id` 字段存在，则要根据 ID 寻找对应的比赛。如果比赛 ID 不存在，返回错误响应；如果比赛 ID 存在，更新信息并返回比赛信息作为响应。

    如果 `id` 字段不存在，则新建比赛并返回比赛信息作为响应。新建的比赛保证其 `id` 不与现有比赛重复。

    在新建或更新比赛的时候，需要检查题目和用户是否都存在。如果不存在，则返回错误响应。

    由于 `id=0` 有特殊用途，因此新建比赛时，生成或用户指定的 `id` 都不能为 0。

    新建比赛时，如果已有比赛，则新比赛 ID 为现有比赛 ID 最大值加一，否则为 1。

    在后续获取比赛信息时，得到的题目 ID（`problem_ids`）和用户 ID（`user_ids`）列表内的元素顺序应当与创建或更新时相同。

=== "响应"

    如果比赛被成功更新或创建，则：

    HTTP 状态码为 200 OK，并附带 JSON 格式的正文。正文的 JSON 应该包括一个字典，描述创建成功或更新后的比赛信息，除了 `id` 变为必选以外，各字段与请求相同。

=== "错误"

    * 传入的 `id` 等于 0：HTTP 400 Bad Request，`reason=ERR_INVALID_ARGUMENT, code=1, message="Invalid contest id"`
    * 请求格式出现错误，或者出现重复的题目或用户 ID：HTTP 400 Bad Request，`reason=ERR_INVALID_ARGUMENT, code=1, message="Invalid argument xxx` 或框架自动检测并生成的错误
    * 根据 ID 找不到比赛，或者比赛中出现了不存在的题目或用户：HTTP 404 Not Found，`reason=ERR_NOT_FOUND, code=3, message="Contest xxx not found."`

### GET /contests

获取比赛列表

=== "请求"

    请求不需要附带参数。

=== "行为"

    以 JSON 响应返回所有比赛，按照 ID 升序排列。

=== "响应"

    HTTP 状态码为 200 OK，并附带 JSON 格式的正文。样例：

    ```json
    [
      {
        "id": 1,
        "name": "Rust Course Project 2",
        "from": "2022-08-27T02:05:29.000Z",
        "to": "2022-08-27T02:05:30.000Z",
        "problem_ids": [
          2,
          1,
          3
        ],
        "user_ids": [
          5,
          4,
          6
        ],
        "submission_limit": 32
      }
    ]
    ```

    正文的 JSON 应该包含一个数组，数组的每一项是一个字典，每个字典对应一个比赛，字典的字段与 `POST /contests` 的响应相同。

### GET /contests/{contestId}

获取单个比赛信息

=== "请求"

    请求应在 URL 路径上传递比赛 id，如：

    ```
    GET http://localhost:12345/contests/1
    ```

    表示查询 ID 为 1 的比赛。ID 不能为 0。


=== "行为"

    根据 URL 中的比赛 id 找到比赛并发送响应。

=== "响应"

    如果找到了比赛，则以 JSON 的形式返回结果，其内容与 `POST /contests` 的响应一致。

=== "错误"

    * 找不到比赛：HTTP 404 Not Found，`reason=ERR_NOT_FOUND, code=3, message="Contest xxx not found."` 或框架自动检测并生成的错误
    * 传入的 `id` 等于 0：HTTP 400 Bad Request，`reason=ERR_INVALID_ARGUMENT, code=1, message="Invalid contest id"`

### GET /contests/{contestId}/ranklist

获取单个比赛的排行榜

=== "请求"

    请求应在 URL 路径上传递比赛 id 和参数，如：

    ```
    GET http://localhost:12345/contests/1/ranklist?scoring_rule=highest&tie_breaker=submission_time
    ```

    表示查询 ID 为 1 的比赛的排行榜。

    * `scoring_rule`：可选（默认为 `latest`），针对同一个用户同一个题目不同提交的评分方式，可能的取值有：`latest`（按最后一次提交算分），`highest`（按分数最高的提交中提交时间最早的提交算分）
    * `tie_breaker`：可选，当有多个用户的分数相同时，用于打破平局的规则，可能的取值有：`submission_time`（每个用户每个题目按照 `scoring_rule` 找到评分所使用的提交，再按**每个用户所有题目评分使用的提交时间的最晚时间**升序，如果用户所有题目一个提交都没有，则取时间无穷晚），`submission_count`（按**总提交数量**升序），`user_id`（按用户 ID 升序）。如果不提供此参数，或者即使提供了此参数，也无法打破平局，则平局的用户赋予相同名次，并按照用户 ID 升序排列。

    下面形式化地定义名次的计算规则：

    定义一个关于用户的全序关系 $(X, \le)$，$X$ 为全体用户，满足以下性质：

    1. 用户 A 分数比用户 B 高，则 $A > B$
    2. 用户 A 分数和分数 B 一样，但是按照同分排序 `tie_breaker` 规则，A 应当名次靠前，则 $A > B$
    3. 用户 A 分数和分数 B 一样，且按照同分排序 `tie_breaker` 规则无法打破平局，则 $A = B$

    定义名次 $n(A) = |\{B | B > A, B \in X\}|+1$，即名次等于全序关系中大于自身的用户数量加一。

=== "行为"

    根据 URL 中的比赛 id 找到比赛，计算排行榜并发送响应。  
    特别地，比赛 id 为 0 总是表示全局排行榜，即包括所有的用户和所有的题目（按题目 id 升序）。

=== "响应"

    以 JSON 的形式返回结果，其内容如下：

    ```json
    [
      {
        "user": {
          "id": 0,
          "name": "root"
        },
        "rank": 1,
        "scores": [
          0,
          100
        ]
      }
    ]
    ```

    JSON 格式的正文是一个数组，数组的每个元素是一个字典，字典有如下的字段：

    1. `user`：必选，用户信息，一个字典，包括用户 ID（`id`）和用户名（`name`）
    2. `rank`：必选，排名，1 表示第一名
    3. `scores`：必选，用户在每个题目中的得分，顺序与比赛信息中 `problem_ids` 对应

=== "错误"

    * 找不到比赛：HTTP 404 Not Found，`reason=ERR_NOT_FOUND, code=3, message="Contest xxx not found."`
    * 请求格式出现错误：HTTP 400 Bad Request，`reason=ERR_INVALID_ARGUMENT, code=1, message="Invalid argument xxx` 或框架自动检测并生成的错误
