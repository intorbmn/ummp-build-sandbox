# Hướng dẫn sử dụng UMMP

UMMP tạo thư mục đầu ra riêng khi port mã nguồn hoặc migrate modpack. Hãy sao lưu dự án, đọc báo cáo và review thay đổi trước khi build. Lệnh chạy thành công không đồng nghĩa mod chắc chắn tương thích trong game.

## 1. Cài đặt

### Windows x64
1. Tải ZIP hoặc MSI Windows từ [GitHub Releases](https://github.com/intorbmn/Universal-Minecraft-Mod-Porter/releases).
2. Giải nén ZIP hoặc chạy MSI. CLI là ummp.exe.
3. Mở PowerShell:

~~~powershell
C:/tools/ummp/ummp.exe --version
C:/tools/ummp/ummp.exe --help
~~~

Bản release candidate chưa ký số, SmartScreen có thể cảnh báo. Hãy kiểm tra checksum và chỉ chạy bản build đáng tin cậy.

### Linux x64 / macOS ARM64
Tải archive tar.gz đúng nền tảng, kiểm tra checksum, giải nén rồi chạy ./ummp --version. Có thể thêm thư mục binary vào PATH.

### Kết nối Claude Code qua MCP
Đăng ký CLI đã giải nén:

~~~powershell
claude mcp add ummp -- C:/tools/ummp/ummp.exe mcp
claude mcp list
~~~

MCP dùng stdio. Hãy để MCP client quản lý tiến trình, không mở nó thành dịch vụ mạng.

## 2. Port mã nguồn mod

UMMP port thư mục mã nguồn hoặc ZIP chứa source, không port được JAR đóng nếu không có source. JAR biên dịch vẫn có thể inspect và scan.

~~~sh
ummp analyze ./MyMod
ummp scan ./MyMod
ummp port ./MyMod --to 1.21.1 --loader neoforge
~~~

Có thể thêm --from 1.20.1, --output ./out/MyMod, --zip, --kb mappings.db hoặc --mappings mappings.json. Dùng ummp port --help để xem đủ tùy chọn.

Mở UMMP_REPORT.md trong thư mục đầu ra. Xử lý mục manual và review từng file thay đổi trước khi build. Chuyển đổi loader hiện chủ yếu mang tính cấu trúc; một số API riêng vẫn cần sửa thủ công.

## 3. Kiểm tra và migrate modpack

Kiểm tra, không ghi file:

~~~sh
ummp pack inspect ./MyPack.mrpack
~~~

Migrate sang Minecraft version và loader đích:

~~~sh
ummp pack migrate ./MyPack.mrpack --to 1.21.1 --loader neoforge --format mrpack,folder
~~~

Thêm --online để tra cứu version trên Modrinth ở chế độ chỉ đọc. Đưa source của mod cần port vào --sources ./mod-sources. UMMP không tải JAR mod; các mục update/replace chưa xử lý sẽ nằm trong báo cáo. Đọc UMMP_PACK_REPORT.md, chú ý risk, dependency thiếu, sai phía client/server và bước thủ công.

## 4. Đọc báo cáo

Kiểm tra version/loader nguồn và đích, file thay đổi và diff, confidence và lý do, cảnh báo và file bị bỏ qua, mục manual, dependency chưa xử lý, risk, kết quả build/test/server nếu có.

Lệnh thành công chỉ xác nhận pipeline đã hoàn thành các bước được định nghĩa, không chứng minh gameplay đúng hoặc mọi API đã được chuyển đổi.

## 5. Backend build

Mặc định UMMP dùng GitHub Actions:

~~~sh
ummp build ./ported-mod --repo TEN_GITHUB_CUA_BAN/ummp-build-sandbox
~~~

Đăng nhập bằng gh auth login và dùng repository do bạn kiểm soát. UMMP tạo branch build và báo cáo build, test, khởi động dedicated server.

Build trên máy hiện tại:

~~~sh
ummp build ./ported-mod --local
~~~

Script build local chạy với quyền user. Ưu tiên GitHub backend với source không đáng tin cậy. Build local mức Medium cần --yes; mức High cần --i-accept-risk. Không bỏ qua refusal nếu chưa hiểu finding và hậu quả.

## 6. Hỗ trợ bằng AI

Thiết lập API key bằng cách nhập ẩn trong terminal:

~~~sh
ummp ai-key set --provider anthropic
ummp ai-key status
ummp ai-assist ./MyMod --from 1.20.1 --to 1.21.1 --loader neoforge --mappings mappings.json
~~~

Đọc từng diff. UMMP chỉ áp dụng thay đổi vượt qua validation và approval, đồng thời tạo checkpoint trước khi ghi. --approve-reviewed chỉ bỏ qua prompt với đề xuất mà review agent đã duyệt, không duyệt đề xuất bị từ chối. AI có thể sai hoặc tạo thay đổi không tương thích.

Để nhận gợi ý từ build/crash log:

~~~sh
ummp ai-debug ./MyMod --log build.log --from 1.20.1 --to 1.21.1 --loader neoforge
~~~

## 7. Rollback và recovery

Khôi phục checkpoint mới nhất sau khi migrate:

~~~sh
ummp rollback ./MyMod
~~~

Nếu migrate bị gián đoạn, dùng session UUID được in ra trước khi lệnh chờ hoàn tất:

~~~sh
ummp recover ./MyMod --session SESSION-UUID
~~~

Phase 13E chỉ hỗ trợ rollback, không tiếp tục/replay migration. Các thư mục bị loại khỏi snapshot như .git, build output và thư mục skip nằm ngoài bảo đảm rollback. Luôn có backup riêng.

## 8. Kiểm tra bản cập nhật

~~~sh
ummp update --check
~~~

Lệnh hiển thị tag và URL release mới hơn, không tự tải hoặc cài đặt. Kiểm tra tự động mặc định tắt. Muốn bật, đặt check_updates = true trong ummp.toml hoặc UMMP_CHECK_UPDATES=true. Khi bật, tối đa một lần mỗi 24 giờ. Lỗi mạng nền được bỏ qua im lặng; không gửi telemetry.

## 9. Xử lý sự cố

### Sai phiên bản Java
Kiểm tra java -version và ./gradlew --version (Windows dùng gradlew.bat --version). Cài JDK phù hợp Minecraft/loader đích, đặt JAVA_HOME và bảo đảm Gradle toolchain trùng khớp. Tham khảo chung: Minecraft 1.18–1.20.4 thường dùng Java 17, 1.20.5 trở lên dùng Java 21; bản cũ hơn có thể cần runtime cũ. Ưu tiên yêu cầu chính xác của loader/dự án.

### Gradle wrapper hoặc build thất bại
Chạy --version của wrapper, kiểm tra file wrapper còn đủ và đọc lỗi compiler/configuration đầu tiên có ý nghĩa. Lưu log trước khi xóa build output. Ưu tiên GitHub build với source không đáng tin cậy vì Gradle script local chạy bằng quyền user.

### Maven Central / HTTP 429
429 là giới hạn tần suất, không nhất thiết là lỗi source. Chờ rồi thử lại, tránh build liên tục và kiểm tra repository mirror hoặc package proxy có xác thực. Không ghi token vào build file hay log.

### Bị từ chối vì risk
Chạy ummp scan PATH, đọc finding và file liên quan. High risk là cổng an toàn, không phải lỗi build. Không bypass chỉ để lệnh chạy thành công. Nếu thực sự chấp nhận rủi ro, chỉ dùng đúng cờ xác nhận được tài liệu hóa và giữ báo cáo.

### Không có source / chỉ có JAR đóng
UMMP có thể inspect và scan JAR, nhưng migrate cần source project. Hãy xin source từ tác giả hoặc để mod ở trạng thái cần xử lý thủ công.

### Không kết nối được GitHub khi update check
Lệnh thủ công hiển thị lỗi. Kiểm tra nền bỏ qua lỗi và không thử lại quá một lần/ngày. Kiểm tra mạng/proxy tới api.github.com; UMMP không cài update ngầm.

## 10. Giới hạn quan trọng

- Chuyển đổi loader mang tính cấu trúc; nhiều API vẫn cần sửa thủ công.
- Kiểm tra server khởi động không thay thế gameplay test.
- Static risk scan không thể chứng minh mod an toàn tuyệt đối.
- JAR đóng không được port và UMMP không tự tải binary mod.
- Recovery chỉ rollback và loại trừ thư mục skip của snapshot.
- Release chưa ký số có thể kích hoạt cảnh báo SmartScreen hoặc Gatekeeper.
